//! §13 diffless-op tests: a `sync`/`prime` runs pre→post against the checkout
//! with no seal, but still reads the checkout tip around `pre` and threads the
//! before/after pair (metadata-less) to `post`. Shares the §8 engine harness
//! ([`super`]: the fakes + `journal`/`plugin` helpers).

use super::*;

/// Run a diffless op through the engine, returning the result and the journal.
fn run_diffless(run_fail: Option<&'static str>, pre: &[&str], post: &[&str]) -> (Result<(), OpError>, Vec<String>) {
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), run_fail);
    let pre: Vec<_> = pre.iter().map(|n| plugin(n)).collect();
    let post: Vec<_> = post.iter().map(|n| plugin(n)).collect();
    let result = Engine::new(&anvil, &plugins, &test_log()).diffless(Verb::Sync, Path::new("/op"), &pre, &post);
    let log = jrn.borrow().clone();
    (result, log)
}

#[test]
fn a_diffless_op_reads_the_tip_around_pre_then_runs_post() {
    let (r, log) = run_diffless(None, &["a"], &["b"]);
    assert!(r.is_ok());
    // §13: read the checkout tip before `pre` and after it, no open/seal/close.
    assert_eq!(log, ["head", "run:a:pre", "head", "run:b:post"]);
}

#[test]
fn a_diffless_post_sees_the_moved_tip_while_pre_sees_none() {
    // §13: `pre` runs before the tip is fact (None); `post` sees the AFTER tip
    // ("T1", the second head read — not the "T0" before `pre`).
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), None);
    let pre = [plugin("a")];
    let post = [plugin("b")];
    Engine::new(&anvil, &plugins, &test_log()).diffless(Verb::Sync, Path::new("/op"), &pre, &post).unwrap();
    assert_eq!(*plugins.seen.borrow(), [("a".into(), None), ("b".into(), Some("T1".into()))]);
}

#[test]
fn a_diffless_abort_unwinds_only_its_plugins() {
    // "a" lands, "b" fails in pre; post never runs; tier 1 is empty. The tip is
    // read once (before `pre`); the after-read never happens.
    let (r, log) = run_diffless(Some("b"), &["a", "b"], &["c"]);
    assert!(matches!(r, Err(OpError::Plugin { ref name, .. }) if name == "b"));
    assert_eq!(log, ["head", "run:a:pre", "run:b:pre", "rollback:a:pre"]);
}

#[test]
fn a_diffless_post_abort_hands_the_moved_facts_to_every_rollback() {
    // Post "c" lands, "d" fails — only succeeded runs unwind (§14). Once the op
    // moved the checkout, EVERY rollback gets the moved facts ("T1"), pre-phase
    // "a" included — the same no-phase-split rule as a sealing op (bl-430e).
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), Some("d"));
    let pre = [plugin("a")];
    let post = [plugin("c"), plugin("d")];
    let _ = Engine::new(&anvil, &plugins, &test_log()).diffless(Verb::Sync, Path::new("/op"), &pre, &post);
    let seen = plugins.seen.borrow().clone();
    assert_eq!(seen.iter().find(|(k, _)| k == "rb:c").unwrap().1, Some("T1".into()));
    assert_eq!(seen.iter().find(|(k, _)| k == "rb:a").unwrap().1, Some("T1".into()));
}

/// Run a `prime` pass through the engine (bl-698d). `moved` = the value `step`
/// (core's materialize) reports the dial moved to (`None` = it held); `step_fail`
/// makes `step` error; `run_fail` fails the named plugin.
fn run_prime(
    moved: Option<&'static str>,
    step_fail: bool,
    run_fail: Option<&'static str>,
    pre: &[&str],
    post: &[&str],
) -> (Result<(), OpError>, Vec<String>) {
    let jrn = journal();
    let anvil = FakeAnvil::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), run_fail);
    let pre: Vec<_> = pre.iter().map(|n| plugin(n)).collect();
    let post: Vec<_> = post.iter().map(|n| plugin(n)).collect();
    let mut step = || -> io::Result<Option<String>> {
        if step_fail {
            return Err(io::Error::other("materialize"));
        }
        Ok(moved.map(String::from))
    };
    let result = Engine::new(&anvil, &plugins, &test_log()).prime(
        Path::new("/landing"),
        Path::new("/store"),
        &pre,
        &post,
        &mut step,
    );
    let log = jrn.borrow().clone();
    (result, log)
}

#[test]
fn a_prime_pass_runs_pre_once_then_post_once() {
    // The conformant shape (the dial held — every plain prime): one pre, one
    // post, no second pass to take (bl-698d).
    let (r, log) = run_prime(None, false, None, &["a"], &["b"]);
    assert!(r.is_ok());
    assert_eq!(log, ["run:a:pre", "run:b:post"]);
}

#[test]
fn a_prime_step_failure_is_a_substrate_abort_that_unwinds_pre() {
    // `step` (core's materialize) fails after `pre` ran → a Substrate abort; the
    // pre plugin rolls back and `post` never runs.
    let (r, log) = run_prime(None, true, None, &["a"], &["b"]);
    assert!(matches!(r, Err(OpError::Substrate(_))));
    assert_eq!(log, ["run:a:pre", "rollback:a:pre"]);
}

#[test]
fn a_prime_whose_pre_moved_the_dial_aborts_on_the_first_pass() {
    // A pre participant rewrote `tasks_branch` (bl-33db's runaway): no conformant
    // plugin moves the dial — config crosses only by install — so the violation
    // is a first-pass ERROR (bl-698d), the message naming the rule and the moved
    // value. The run pre unwinds; `post` never runs.
    let (r, log) = run_prime(Some("balls/elsewhere"), false, None, &["a"], &["b"]);
    let Err(OpError::Substrate(e)) = r else { panic!("expected a Substrate abort, got {r:?}") };
    let msg = e.to_string();
    assert!(msg.contains("prime.pre may not move tasks_branch"), "{msg}");
    assert!(msg.contains("balls/elsewhere"), "{msg}"); // the moved value, named
    assert_eq!(log, ["run:a:pre", "rollback:a:pre"]);
}

#[test]
fn a_prime_post_abort_unwinds_every_run_plugin_in_reverse() {
    // pre "a" lands, the dial held, post "b" fails → the whole op unwinds, and
    // the abort renders as the catalog's E7 — "plugin failed during prime,
    // rolled back K prior" (K = 1, the run "a") — not the generic plugin abort
    // (bl-3ddb).
    let (r, log) = run_prime(None, false, Some("b"), &["a"], &["b"]);
    let Err(e) = r else { panic!("expected a Plugin abort") };
    assert!(matches!(e, OpError::Plugin { ref name, .. } if name == "b"));
    assert!(
        e.to_string().starts_with("plugin failed during prime, rolled back 1 prior:"),
        "{e}"
    );
    assert_eq!(log, ["run:a:pre", "run:b:post", "rollback:a:pre"]);
}
