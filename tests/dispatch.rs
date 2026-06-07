//! End-to-end harness: build the `bl` binary and run it from a throwaway temp
//! directory, never against the dev repo's own task list. The read verbs
//! (`show`/`list`/`ready`/`dep-tree`, §9) render the store; the still-unwired
//! diffless verbs (`doctor`/`install`) report their §8 op plan; the
//! checkout-lifecycle verbs (`prime`/`sync`, §12/§13) and the deliverable verbs
//! (`create`/`claim`/`close`, §9) run the real engine + the shipped `tracker`
//! sibling end to end.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The freshly-built `bl`, pinned to run inside an isolated temp dir.
fn bl(workspace: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(workspace.path());
    cmd
}

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under `home`
/// and `state` so its clone bundle + stealth lock land in the tempdir, not the
/// real `$HOME`. The `tracker` sibling is found beside the built `bl` (§12).
fn bl_primed(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// Where the tracker's stealth lock lands for an invocation at `project` —
/// `$XDG_STATE_HOME/balls/clones/<pct-enc-project>/stealth.lock`.
fn stealth_lock(state: &Path, project: &Path) -> PathBuf {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(project)
        .root()
        .join("stealth.lock")
}

#[test]
fn an_unwired_diffless_verb_reports_its_op_plan() {
    // doctor is not yet wired into the read path, so it still prints its §8 plan.
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("doctor")
        .assert()
        .success()
        .stdout(contains("doctor: pre -> post"));
}

#[test]
fn a_read_verb_on_an_unprimed_checkout_is_an_empty_success() {
    // No store yet ⇒ the silent-empty case (§13): an empty render, not an error.
    let workspace = TempDir::new().unwrap();
    bl(&workspace).arg("list").assert().success().stdout("");
}

#[test]
fn an_unknown_verb_exits_with_a_usage_error() {
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("frobnicate")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("usage: bl <verb>"));
}

#[test]
fn prime_founds_a_stealth_landing_and_runs_the_tracker_chain() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();

    // Fresh box, no remote → prime founds the landing AND runs the prime chain,
    // whose tracker handler writes the stealth self-lock (§12). Its presence is
    // proof the full engine→subprocess→tracker path ran, not just bootstrap.
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    assert!(stealth_lock(&state, &project).is_file());

    // Idempotent: a second prime converges to a no-op, and `sync` runs the
    // tracker's sync/pre against the store.
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state).arg("sync").assert().success();
}

#[test]
fn create_seals_a_ball_and_runs_the_mutating_post_chain() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();

    bl_primed(&project, &home, &state).arg("prime").assert().success();
    // A real deliverable op: author → seal → the tracker's mutating post (which
    // no-ops on a stealth binding). Its success proves the whole engine→
    // subprocess→tracker path runs for a mutating verb, not just prime/sync.
    bl_primed(&project, &home, &state)
        .args(["create", "A real task", "--as", "me"])
        .assert()
        .success();

    // The ball file landed on the STORE terminus.
    let tasks = balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(&project)
        .store()
        .join("tasks");
    let count = std::fs::read_dir(&tasks).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some()).count();
    assert_eq!(count, 1, "create sealed exactly one ball file");
}

#[test]
fn the_read_verbs_render_a_created_ball_end_to_end() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state)
        .args(["create", "Render me", "-p", "1", "--as", "me"])
        .assert()
        .success();

    // `list` (piped ⇒ non-tty ⇒ plain) shows the ready ball; `ready` agrees.
    bl_primed(&project, &home, &state)
        .arg("list")
        .assert()
        .success()
        .stdout(contains("ready").and(contains("Render me")).and(contains("p1")));
    bl_primed(&project, &home, &state).arg("ready").assert().success().stdout(contains("Render me"));

    // `list --json` is a valid one-element array whose timestamp is ISO-8601.
    let out = bl_primed(&project, &home, &state).args(["list", "--json"]).assert().success();
    let json = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v[0]["title"], "Render me");
    assert!(v[0]["created"].as_str().unwrap().ends_with('Z'));
}
