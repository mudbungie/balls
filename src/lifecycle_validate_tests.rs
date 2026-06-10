//! bl-528c seal-validation tests: a `pre` plugin edit that leaves ANY changed
//! `tasks/*.md` unparseable must abort the op BEFORE the seal — naming the file
//! and the last pre plugin — instead of committing corruption the read verbs
//! then choke on. Shares the §8 engine harness ([`super`]: the fakes +
//! `journal`/`plugin` helpers).

use super::*;
use tempfile::TempDir;

const CORRUPT: &str = "+++\ntitle = \"x\"\ntitle = \"x\"\n+++\n"; // duplicate key

/// Drive a seal whose anvil reports `changed` paths against a real change dir.
fn seal_with_changed(dir: &Path, changed: &[&str], pre: &[&str]) -> (Result<String, OpError>, Vec<String>) {
    let jrn = journal();
    let mut anvil = FakeAnvil::new(jrn.clone(), None);
    anvil.changed_files = changed.iter().map(ToString::to_string).collect();
    let plugins = FakePlugins::new(jrn.clone(), None);
    let base = FakeBase { j: jrn.clone(), fail_stage: false, fail_finalize: false };
    let pre: Vec<_> = pre.iter().map(|n| plugin(n)).collect();
    let result = Engine::new(&anvil, &plugins, &test_log()).seal(&base, Verb::Update, dir, &pre, &[]);
    let log = jrn.borrow().clone();
    (result, log)
}

#[test]
fn a_pre_corrupted_sibling_ball_aborts_before_the_seal() {
    // The bug's first limb: a create/pre plugin edits a SIBLING tasks/*.md and
    // breaks its TOML — the seal must refuse, not commit two files one of
    // which no read can parse.
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir(tmp.path().join("tasks")).unwrap();
    std::fs::write(tmp.path().join("tasks/bl-sib.md"), CORRUPT).unwrap();

    let (r, log) = seal_with_changed(tmp.path(), &["tasks/bl-new.md", "tasks/bl-sib.md"], &["p", "q"]);
    let err = r.unwrap_err();
    assert!(matches!(err, OpError::Invalid(_)));
    let msg = err.to_string();
    assert!(msg.contains("tasks/bl-sib.md"), "names the file: {msg}");
    assert!(msg.contains("pre plugin q"), "names the LAST pre plugin: {msg}");
    assert!(msg.contains("duplicate"), "carries the parse error: {msg}");
    // Aborted pre-seal: pre rolled back in reverse, no head/finalize/seal, the
    // worktree discarded. (`tasks/bl-new.md` is absent — a deletion-shaped
    // entry — so only the corrupt sibling trips the guard.)
    assert_eq!(
        log,
        ["open", "stage", "run:p:pre", "run:q:pre", "rollback:q:pre", "rollback:p:pre", "close"]
    );
}

#[test]
fn a_plugin_free_corruption_is_still_refused_without_a_name() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir(tmp.path().join("tasks")).unwrap();
    std::fs::write(tmp.path().join("tasks/bl-x.md"), "no frontmatter at all").unwrap();

    let (r, _log) = seal_with_changed(tmp.path(), &["tasks/bl-x.md"], &[]);
    let msg = r.unwrap_err().to_string();
    assert!(msg.contains("tasks/bl-x.md does not parse"), "no plugin to name: {msg}");
    assert!(msg.contains("frontmatter"), "carries the parse error: {msg}");
}

#[test]
fn parseable_deleted_and_non_ball_changes_seal_cleanly() {
    // The guard's three pass-throughs: a non-ball path (no §3 schema), a
    // deleted ball (close — nothing to parse), and a changed ball that parses.
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir(tmp.path().join("tasks")).unwrap();
    let ok = crate::task::Task { title: "fine".into(), ..Default::default() };
    crate::taskfile::write_task(tmp.path(), "bl-ok", &ok).unwrap();

    let (r, log) = seal_with_changed(tmp.path(), &["notes.txt", "tasks/bl-gone.md", "tasks/bl-ok.md"], &["p"]);
    assert_eq!(r.unwrap(), "C1");
    assert_eq!(log, ["open", "stage", "run:p:pre", "head", "finalize", "seal", "close"]);
}

#[test]
fn a_failing_changed_read_aborts_as_an_anvil_error() {
    let (r, log) = run_seal(Some("changed"), false, false, None, &[], &[]);
    assert!(matches!(r, Err(OpError::Anvil(_))));
    assert_eq!(log, ["open", "stage", "close"]);
}
