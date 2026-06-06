//! End-to-end harness: build the `bl` binary and run it from a throwaway temp
//! directory, never against the dev repo's own task list. The read verbs still
//! report their §8 op plan; the checkout-lifecycle verbs (`prime`/`sync`,
//! §12/§13) and the deliverable verbs (`create`/`claim`/`close`, §9) run the real
//! engine + the shipped `tracker` sibling end to end.

use assert_cmd::Command;
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
fn dispatches_a_known_verb_to_its_op_plan() {
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("dep-tree")
        .assert()
        .success()
        .stdout(contains("dep-tree: pre -> post"));
}

#[test]
fn a_diffless_verb_skips_the_seal() {
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("show")
        .assert()
        .success()
        .stdout(contains("show: pre -> post"));
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

    // Idempotent: a second prime converges to a no-op, and `sync` walks the
    // (length-1) trail and runs the tracker's sync/pre against the terminus.
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

    // The ball file landed on the operating terminus.
    let tasks = balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(&project)
        .operating()
        .join("tasks");
    let count = std::fs::read_dir(&tasks).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some()).count();
    assert_eq!(count, 1, "create sealed exactly one ball file");
}
