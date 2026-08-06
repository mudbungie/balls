//! End-to-end `bl reopen` — the full create → claim → close → reopen round trip
//! through the real engine, store and plugin chain, against a throwaway temp
//! project (a real git repo on `main`, so the delivery plugin can fork
//! `work/<id>` worktrees). Proves the restored ball is LIVE again — visible to
//! `bl list`, claimable, and no longer in the dead set — which the unit tests,
//! running on the authoring side alone, cannot show.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under `home`
/// and `state` so its clone bundle lands in the tempdir, not the real `$HOME`.
fn bl_primed(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A real project repo on `main` with a seed commit, plus a primed checkout.
fn primed_project(tmp: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// The id `bl create` printed alone to stdout (§9).
fn created_id(out: assert_cmd::assert::Assert) -> String {
    String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// Create a ball, claim it as `who`, then close it — a ball retired the ordinary
/// way, with the claimant a close leaves in the last live version of the file.
fn closed_ball(project: &Path, home: &Path, state: &Path, who: &str) -> String {
    let bl = || bl_primed(project, home, state);
    let id = created_id(bl().args(["create", "A retired ball", "-p", "3", "-t", "bug"]).assert().success());
    bl().args(["claim", &id, "--as", who]).assert().success();
    bl().args(["close", &id, "--as", who]).assert().success();
    id
}

#[test]
fn reopen_restores_a_closed_ball_to_the_live_set() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let bl = || bl_primed(&project, &home, &state);
    let id = closed_ball(&project, &home, &state, "ghost");

    // Closed: absence is the record, so the live listing has lost it.
    bl().arg("list").assert().success().stdout(contains(&id).not());

    bl().args(["reopen", &id, "--as", "alice"]).assert().success().stderr(contains(format!("reopen {id}")));

    // Live again — and carrying everything it died with.
    bl().arg("list").assert().success().stdout(contains(&id).and(contains("A retired ball")));
    let json = bl().args(["show", &id, "--json"]).assert().success();
    let out = String::from_utf8(json.get_output().stdout.clone()).unwrap();
    assert!(out.contains("\"priority\": 3"), "priority survived the round trip: {out}");
    assert!(out.contains("\"bug\""), "tags survived the round trip: {out}");
    // Verbatim by default: the claimant the close left behind comes back.
    assert!(out.contains("\"claimant\": \"ghost\""), "claimant restored verbatim: {out}");

    // …and it is no longer in the dead set: one incarnation, and it is live.
    bl().args(["list", "--status", "closed"]).assert().success().stdout(contains(&id).not());
}

#[test]
fn reopen_clean_restores_the_ball_ready_to_claim() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let bl = || bl_primed(&project, &home, &state);
    let id = closed_ball(&project, &home, &state, "ghost");

    bl().args(["reopen", &id, "--clean", "--as", "alice"]).assert().success();

    // The stale claim is gone, so the ball is on the ready list and a fresh
    // claim succeeds — where a verbatim restore would refuse as already-claimed.
    bl().args(["list", "--status", "ready"]).assert().success().stdout(contains(&id));
    bl().args(["claim", &id, "--as", "alice"]).assert().success();
}

#[test]
fn a_verbatim_reopen_leaves_the_ball_claimed_by_the_closer() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let bl = || bl_primed(&project, &home, &state);
    let id = closed_ball(&project, &home, &state, "ghost");

    bl().args(["reopen", &id, "--as", "alice"]).assert().success();

    // The restored claim is a real claim: it refuses the next claimant, and
    // `bl unclaim` is the in-band fix (the same field `--clean` would drop).
    bl().args(["claim", &id, "--as", "alice"]).assert().failure().stderr(contains("already claimed by ghost"));
    bl().args(["unclaim", &id, "--as", "alice"]).assert().success();
    bl().args(["claim", &id, "--as", "alice"]).assert().success();
}

#[test]
fn reopen_refuses_a_live_id_and_an_unknown_one() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let bl = || bl_primed(&project, &home, &state);
    let live = created_id(bl().args(["create", "still open"]).assert().success());

    bl().args(["reopen", &live]).assert().failure().stderr(contains(format!("{live} is live")));
    bl().args(["reopen", "bl-none"]).assert().failure().stderr(contains("bl-none names nothing"));
    // The refusals write nothing: the live ball is untouched.
    bl().args(["show", &live]).assert().success().stdout(contains("still open"));
}

#[test]
fn a_reopened_ball_closes_again() {
    // Nothing counts closes per ball: the second retirement is ordinary, and it
    // leaves the id dead once more (the newest deletion is the one that counts).
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let bl = || bl_primed(&project, &home, &state);
    let id = closed_ball(&project, &home, &state, "ghost");

    bl().args(["reopen", &id, "--clean", "--as", "alice"]).assert().success();
    bl().args(["claim", &id, "--as", "alice"]).assert().success();
    bl().args(["close", &id, "--as", "alice"]).assert().success();

    bl().arg("list").assert().success().stdout(contains(&id).not());
    bl().args(["list", "--status", "closed"]).assert().success().stdout(contains(&id));
}

#[test]
fn reopen_journals_its_note_and_carries_the_op_trailer() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let bl = || bl_primed(&project, &home, &state);
    let id = closed_ball(&project, &home, &state, "ghost");

    bl().args(["reopen", &id, "--as", "alice", "-m", "the fix regressed"]).assert().success();

    // The journal is the store branch's history (§9), so the restore and its
    // note read back on `bl show` like any other op in the ball's life.
    bl().args(["show", &id]).assert().success().stdout(contains("reopen").and(contains("the fix regressed")));
}
