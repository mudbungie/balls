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

/// Run a `prime` fixpoint through the engine (bl-0a23). `passes` = how many `pre`
/// passes before `step` (core's materialize) reports converged; `step_fail` makes
/// `step` error on its first call; `run_fail` fails the named plugin.
fn run_fixpoint(
    passes: u32,
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
    let mut n = 0u32;
    let mut step = || -> io::Result<Option<String>> {
        if step_fail {
            return Err(io::Error::other("materialize"));
        }
        n += 1;
        // Converge once `step` has run `passes` times; until then the dial moved.
        Ok((n < passes).then(|| format!("dial-{n}")))
    };
    let result = Engine::new(&anvil, &plugins, &test_log()).fixpoint(
        Verb::Prime,
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
fn a_fixpoint_runs_pre_until_step_converges_then_runs_post_once() {
    // Two passes: pre, step(false), pre, step(true), then post. §12 "1 extra pass".
    let (r, log) = run_fixpoint(2, false, None, &["a"], &["b"]);
    assert!(r.is_ok());
    assert_eq!(log, ["run:a:pre", "run:a:pre", "run:b:post"]);
}

#[test]
fn a_fixpoint_converging_on_the_first_pass_runs_pre_once() {
    // The common case (the dial held): exactly one pre, then post — no extra pass.
    let (r, log) = run_fixpoint(1, false, None, &["a"], &["b"]);
    assert!(r.is_ok());
    assert_eq!(log, ["run:a:pre", "run:b:post"]);
}

#[test]
fn a_fixpoint_step_failure_is_a_substrate_abort_that_unwinds_pre() {
    // `step` (core's materialize) fails after `pre` ran → a Substrate abort; the
    // pre plugin rolls back and `post` never runs.
    let (r, log) = run_fixpoint(1, true, None, &["a"], &["b"]);
    assert!(matches!(r, Err(OpError::Substrate(_))));
    assert_eq!(log, ["run:a:pre", "rollback:a:pre"]);
}

#[test]
fn a_fixpoint_whose_dial_never_holds_aborts_loudly_at_the_pass_cap() {
    // A pre participant rewrites the dial on EVERY pass (bl-33db): the loop must
    // not spin forever — it aborts at FIXPOINT_CAP, the error naming the op and
    // the last dial value, and unwinds every run pre. `post` never runs.
    let (r, log) = run_fixpoint(u32::MAX, false, None, &["a"], &["b"]);
    let Err(OpError::Substrate(e)) = r else { panic!("expected a Substrate abort, got {r:?}") };
    let msg = e.to_string();
    assert!(msg.contains(&format!("fixpoint pass cap ({FIXPOINT_CAP})")), "{msg}");
    assert!(msg.contains("prime.pre"), "{msg}");
    assert!(msg.contains(&format!("dial-{FIXPOINT_CAP}")), "{msg}"); // the value still moving at the cap
    let cap = FIXPOINT_CAP as usize;
    assert_eq!(log.iter().filter(|l| *l == "run:a:pre").count(), cap);
    assert_eq!(log.iter().filter(|l| *l == "rollback:a:pre").count(), cap);
    assert!(!log.contains(&"run:b:post".to_string()));
}

#[test]
fn a_fixpoint_post_abort_unwinds_every_run_plugin_in_reverse() {
    // pre "a" lands, step converges, post "b" fails → the whole op unwinds.
    let (r, log) = run_fixpoint(1, false, Some("b"), &["a"], &["b"]);
    assert!(matches!(r, Err(OpError::Plugin { ref name, .. }) if name == "b"));
    assert_eq!(log, ["run:a:pre", "run:b:post", "rollback:a:pre"]);
}
