//! §13 diffless-op tests: a `sync`/`prime` runs pre→post against `operating/`
//! with no seal, but still reads the operating tip around `pre` and threads the
//! before/after pair (metadata-less) to `post`. Shares the §8 engine harness
//! ([`super`]: the fakes + `journal`/`plugin` helpers).

use super::*;

/// Run a diffless op through the engine, returning the result and the journal.
fn run_diffless(run_fail: Option<&'static str>, pre: &[&str], post: &[&str]) -> (Result<(), OpError>, Vec<String>) {
    let jrn = journal();
    let term = FakeTerminus::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), run_fail);
    let pre: Vec<_> = pre.iter().map(|n| plugin(n)).collect();
    let post: Vec<_> = post.iter().map(|n| plugin(n)).collect();
    let result = Engine::new(&term, &plugins).diffless(Verb::Sync, Path::new("/op"), &pre, &post);
    let log = jrn.borrow().clone();
    (result, log)
}

#[test]
fn a_diffless_op_reads_the_tip_around_pre_then_runs_post() {
    let (r, log) = run_diffless(None, &["a"], &["b"]);
    assert!(r.is_ok());
    // §13: read the operating tip before `pre` and after it, no open/seal/close.
    assert_eq!(log, ["head", "run:a:pre", "head", "run:b:post"]);
}

#[test]
fn a_diffless_post_sees_the_moved_tip_while_pre_sees_none() {
    // §13: `pre` runs before the tip is fact (None); `post` sees the AFTER tip
    // ("T1", the second head read — not the "T0" before `pre`).
    let jrn = journal();
    let term = FakeTerminus::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), None);
    let pre = [plugin("a")];
    let post = [plugin("b")];
    Engine::new(&term, &plugins).diffless(Verb::Sync, Path::new("/op"), &pre, &post).unwrap();
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
fn a_diffless_post_abort_hands_the_moved_facts_to_post_rollback() {
    // Post "c" lands, "d" fails — only succeeded runs unwind (§14), so "c"'s post
    // rollback sees the AFTER tip ("T1") and "a"'s pre rollback sees none (§7/§13).
    let jrn = journal();
    let term = FakeTerminus::new(jrn.clone(), None);
    let plugins = FakePlugins::new(jrn.clone(), Some("d"));
    let pre = [plugin("a")];
    let post = [plugin("c"), plugin("d")];
    let _ = Engine::new(&term, &plugins).diffless(Verb::Sync, Path::new("/op"), &pre, &post);
    let seen = plugins.seen.borrow().clone();
    assert_eq!(seen.iter().find(|(k, _)| k == "rb:c").unwrap().1, Some("T1".into()));
    assert_eq!(seen.iter().find(|(k, _)| k == "rb:a").unwrap().1, None);
}
