//! §8/§14 engine tests — fakes for the three seams ([`Anvil`],
//! [`BaseChange`], [`Plugins`]) share one journal so a single sequence assertion
//! proves both WHAT ran and the ORDER (plugins roll back before core un-seals).

use super::*;
use crate::message::Message;
use std::io;
use std::path::Path;

// The journaling fakes for the three seams live in a sibling module, shared
// with the diffless/narration/seal-validation test modules below.
#[path = "lifecycle_test_fakes.rs"]
mod fakes;
pub(crate) use fakes::*;

/// Run a mutating op through the engine, returning the result and the journal.
pub(crate) fn run_seal(
    anvil_fail: Option<&'static str>,
    fail_stage: bool,
    fail_finalize: bool,
    run_fail: Option<&'static str>,
    pre: &[&str],
    post: &[&str],
) -> (Result<String, OpError>, Vec<String>) {
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), anvil_fail);
    let plugins = FakePlugins::new(jrn.clone(), run_fail);
    let base = FakeBase { j: jrn.clone(), fail_stage, fail_finalize };
    let pre: Vec<_> = pre.iter().map(|n| plugin(n)).collect();
    let post: Vec<_> = post.iter().map(|n| plugin(n)).collect();
    let result = Engine::new(&anvil, &plugins, &test_log()).seal(&base, Verb::Close, Path::new("/c"), &pre, &post);
    let log = jrn.borrow().clone();
    (result, log)
}

#[test]
fn a_clean_mutating_op_runs_author_pre_seal_post_then_teardown() {
    let (r, log) = run_seal(None, false, false, None, &["a"], &["b"]);
    assert_eq!(r.unwrap(), "C1");
    assert_eq!(log, ["open", "stage", "run:a:pre", "head", "finalize", "seal", "run:b:post", "close"]);
}

#[test]
fn a_pre_abort_discards_the_worktree_and_never_un_seals() {
    // The pre plugin "a" fails; "z" never runs; nothing sealed.
    let (r, log) = run_seal(None, false, false, Some("a"), &["a", "z"], &[]);
    assert!(matches!(r, Err(OpError::Plugin { ref name, .. }) if name == "a"));
    assert_eq!(log, ["open", "stage", "run:a:pre", "close"]);
}

#[test]
fn a_post_abort_unwinds_every_plugin_in_reverse_then_un_seals() {
    // Pre "a","b" land; the seal lands; post "c" fails — the WHOLE op unwinds:
    // every run plugin rolls back in reverse, THEN core resets the anvil.
    let (r, log) = run_seal(None, false, false, Some("c"), &["a", "b"], &["c"]);
    assert!(matches!(r, Err(OpError::Plugin { ref name, .. }) if name == "c"));
    assert_eq!(
        log,
        [
            "open", "stage", "run:a:pre", "run:b:pre", "head", "finalize", "seal",
            "run:c:post", "rollback:b:pre", "rollback:a:pre", "unseal:T0", "close"
        ]
    );
}

#[test]
fn a_failed_open_leaves_nothing_to_unwind_or_tear_down() {
    let (r, log) = run_seal(Some("open"), false, false, None, &[], &[]);
    assert!(matches!(r, Err(OpError::Anvil(_))));
    assert_eq!(log, ["open"]); // not opened ⇒ no close, no unwind
}

#[test]
fn a_stage_failure_aborts_before_any_plugin_runs() {
    let (r, log) = run_seal(None, true, false, None, &["a"], &[]);
    assert!(matches!(r, Err(OpError::Author(_))));
    assert_eq!(log, ["open", "stage", "close"]);
}

#[test]
fn a_head_failure_aborts_the_seal_pre_boundary() {
    let (r, log) = run_seal(Some("head"), false, false, None, &[], &[]);
    assert!(matches!(r, Err(OpError::Anvil(_))));
    assert_eq!(log, ["open", "stage", "head", "close"]);
}

#[test]
fn a_finalize_failure_aborts_before_the_seal() {
    let (r, log) = run_seal(None, false, true, None, &[], &[]);
    assert!(matches!(r, Err(OpError::Author(_))));
    assert_eq!(log, ["open", "stage", "head", "finalize", "close"]);
}

#[test]
fn a_seal_failure_discards_the_worktree_without_un_sealing() {
    let (r, log) = run_seal(Some("seal"), false, false, None, &[], &[]);
    assert!(matches!(r, Err(OpError::Anvil(_))));
    assert_eq!(log, ["open", "stage", "head", "finalize", "seal", "close"]);
}

#[test]
fn the_seal_commits_the_finalized_5_message() {
    struct MsgBase;
    impl BaseChange for MsgBase {
        fn stage(&self, _dir: &Path) -> io::Result<()> {
            Ok(())
        }
        fn finalize(&self, _dir: &Path) -> io::Result<String> {
            Message {
                verb: Verb::Create,
                actor: "me".into(),
                id: Some("bl-1234".into()),
                subject: "t".into(),
                body: None,
            }
            .render()
        }
    }
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn, None);
    Engine::new(&anvil, &plugins, &test_log()).seal(&MsgBase, Verb::Create, Path::new("/c"), &[], &[]).unwrap();
    assert!(anvil.sealed_msg.borrow().as_deref().unwrap().contains("bl-id: bl-1234"));
}

#[test]
fn post_sees_the_sealed_commit_while_pre_sees_none() {
    // Pre "a" runs before the seal (no facts); post "b" after (commit C1). §7.
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), None);
    let base = FakeBase { j: jrn, fail_stage: false, fail_finalize: false };
    let pre = [plugin("a")];
    let post = [plugin("b")];
    Engine::new(&anvil, &plugins, &test_log())
        .seal(&base, Verb::Close, Path::new("/c"), &pre, &post)
        .unwrap();
    assert_eq!(
        *plugins.seen.borrow(),
        [("a".into(), None), ("b".into(), Some("C1".into()))]
    );
}

#[test]
fn a_post_abort_hands_the_sealed_facts_to_every_rollback() {
    // Post "c" lands, then "d" fails — only SUCCEEDED runs unwind (§14). The op
    // sealed, so EVERY rollback gets the C1 facts, pre-phase "a" included: §14's
    // id rule is "post/rollback from the sealed §5 trailer", and post-seal the
    // change worktree is clean — a starved pre rollback (historically the
    // delivery un-squash) could not re-derive its id and silently no-oped
    // (bl-430e).
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), Some("d"));
    let base = FakeBase { j: jrn, fail_stage: false, fail_finalize: false };
    let pre = [plugin("a")];
    let post = [plugin("c"), plugin("d")];
    let _ = Engine::new(&anvil, &plugins, &test_log()).seal(&base, Verb::Close, Path::new("/c"), &pre, &post);
    let seen = plugins.seen.borrow().clone();
    assert_eq!(seen.iter().find(|(k, _)| k == "rb:c").unwrap().1, Some("C1".into()));
    assert_eq!(seen.iter().find(|(k, _)| k == "rb:a").unwrap().1, Some("C1".into()));
}

#[test]
fn op_error_renders_each_variant_and_is_an_error() {
    let author = OpError::Author(ioerr("x"));
    let anvil = OpError::Anvil(ioerr("y"));
    let substrate = OpError::Substrate(ioerr("m"));
    // The Plugin source already names the locus (`crate::plugin` renders
    // "plugin p aborted the op (…)"); Display must NOT re-prefix it — the
    // stuttered "plugin p aborted the op: plugin p aborted the op" (bl-3ddb).
    let plugin = OpError::Plugin {
        name: "p".into(),
        source: ioerr("plugin p aborted the op (exit status: 1)"),
    };
    assert!(author.to_string().contains("authoring the base change failed"));
    assert!(anvil.to_string().contains("sealing onto the anvil failed"));
    assert!(substrate.to_string().contains("materializing the store failed"));
    assert_eq!(plugin.to_string(), "plugin p aborted the op (exit status: 1)");
    assert!(format!("{author:?}").contains("Author"));
    let _: &dyn std::error::Error = &plugin;
}

// §13 diffless-op tests share this module's engine harness (fakes + helpers).
#[path = "lifecycle_diffless_tests.rs"]
mod diffless;

// bl-cf93 narration-vs-no-op-seal tests share the same harness.
#[path = "lifecycle_narration_tests.rs"]
mod narration;

// bl-528c seal-validation tests share the same harness.
#[path = "lifecycle_validate_tests.rs"]
mod validate;
