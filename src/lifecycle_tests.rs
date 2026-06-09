//! §8/§14 engine tests — fakes for the three seams ([`Anvil`],
//! [`BaseChange`], [`Plugins`]) share one journal so a single sequence assertion
//! proves both WHAT ran and the ORDER (plugins roll back before core un-seals).

use super::*;
use crate::message::Message;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

type Journal = Rc<RefCell<Vec<String>>>;

fn journal() -> Journal {
    Rc::new(RefCell::new(Vec::new()))
}

/// A throwaway log sink for the engine harness: an unwritable path (records are
/// best-effort, so the open fails harmlessly) at the `Error` threshold so the
/// info-level begin/seal records stay quiet — the call sites still execute, and
/// `log_tests` covers the record internals. The engine's logging is tested as
/// behaviour here only through it not perturbing the journaled op sequence.
fn test_log() -> crate::log::Log {
    crate::log::Log::new(Path::new("/nonexistent-balls-test/log").into(), crate::log::Level::Error, Verb::Close, || 0)
}

fn ioerr(what: &str) -> io::Error {
    io::Error::other(what.to_string())
}

fn plugin(name: &str) -> PluginRef {
    PluginRef { name: name.to_string(), bin: None }
}

/// An [`Anvil`] that journals each act, fails the named one, and captures the
/// sealed commit message so a test can assert the §5 trailer landed.
struct FakeAnvil {
    j: Journal,
    fail: Option<&'static str>,
    sealed_msg: RefCell<Option<String>>,
    heads: RefCell<u32>,
}

impl FakeAnvil {
    fn new(j: Journal, fail: Option<&'static str>) -> Self {
        Self { j, fail, sealed_msg: RefCell::new(None), heads: RefCell::new(0) }
    }
    fn log(&self, s: String) {
        self.j.borrow_mut().push(s);
    }
    fn gate(&self, m: &'static str) -> io::Result<()> {
        if self.fail == Some(m) { Err(ioerr(m)) } else { Ok(()) }
    }
}

impl Anvil for FakeAnvil {
    fn head(&self) -> io::Result<String> {
        self.log("head".into());
        self.gate("head")?;
        // A distinct tip per read so a diffless before/after pair differs (T0,
        // T1, …); the seal path reads head once, so it stays "T0".
        let n = *self.heads.borrow();
        *self.heads.borrow_mut() += 1;
        Ok(format!("T{n}"))
    }
    fn open(&self, _dir: &Path) -> io::Result<()> {
        self.log("open".into());
        self.gate("open")
    }
    fn seal(&self, _dir: &Path, message: &str) -> io::Result<String> {
        self.log("seal".into());
        self.gate("seal")?;
        *self.sealed_msg.borrow_mut() = Some(message.to_string());
        Ok("C1".into())
    }
    fn unseal(&self, sha: &str) -> io::Result<()> {
        self.log(format!("unseal:{sha}"));
        self.gate("unseal")
    }
    fn close(&self, _dir: &Path) -> io::Result<()> {
        self.log("close".into());
        self.gate("close")
    }
}

/// A [`Plugins`] that journals every run/rollback and fails the named plugin.
/// `seen` captures the `Sealed` commit each call observed so a test can prove the
/// §7 post facts cross the seam (`None` on every `pre` call).
struct FakePlugins {
    j: Journal,
    fail: Option<&'static str>,
    seen: RefCell<Vec<(String, Option<String>)>>,
}

impl FakePlugins {
    fn new(j: Journal, fail: Option<&'static str>) -> Self {
        Self { j, fail, seen: RefCell::new(Vec::new()) }
    }
}

impl Plugins for FakePlugins {
    fn run(&self, p: &PluginRef, _op: Verb, phase: Phase, _dir: &Path, sealed: Option<&Sealed>) -> io::Result<()> {
        self.j.borrow_mut().push(format!("run:{}:{}", p.name, phase.token()));
        self.seen.borrow_mut().push((p.name.clone(), sealed.map(|s| s.commit.to_string())));
        if self.fail == Some(p.name.as_str()) { Err(ioerr(&p.name)) } else { Ok(()) }
    }
    fn rollback(&self, p: &PluginRef, _op: Verb, phase: Phase, _dir: &Path, sealed: Option<&Sealed>) {
        self.j.borrow_mut().push(format!("rollback:{}:{}", p.name, phase.token()));
        self.seen.borrow_mut().push((format!("rb:{}", p.name), sealed.map(|s| s.commit.to_string())));
    }
}

/// A [`BaseChange`] that journals stage/finalize and fails on demand.
struct FakeBase {
    j: Journal,
    fail_stage: bool,
    fail_finalize: bool,
}

impl BaseChange for FakeBase {
    fn stage(&self, _dir: &Path) -> io::Result<()> {
        self.j.borrow_mut().push("stage".into());
        if self.fail_stage { Err(ioerr("stage")) } else { Ok(()) }
    }
    fn finalize(&self, _dir: &Path) -> io::Result<String> {
        self.j.borrow_mut().push("finalize".into());
        if self.fail_finalize { Err(ioerr("finalize")) } else { Ok("MSG".into()) }
    }
}

/// Run a mutating op through the engine, returning the result and the journal.
fn run_seal(
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
    // change worktree is clean — a starved pre rollback (the delivery un-squash)
    // could not re-derive its id and silently no-oped (bl-430e).
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
    let plugin = OpError::Plugin { name: "p".into(), source: ioerr("z") };
    assert!(author.to_string().contains("authoring the base change failed"));
    assert!(anvil.to_string().contains("sealing onto the anvil failed"));
    assert!(substrate.to_string().contains("materializing the store failed"));
    assert!(plugin.to_string().contains("plugin p aborted the op"));
    assert!(format!("{author:?}").contains("Author"));
    let _: &dyn std::error::Error = &plugin;
}

// §13 diffless-op tests share this module's engine harness (fakes + helpers).
#[path = "lifecycle_diffless_tests.rs"]
mod diffless;
