//! End-to-end harness for §12/§15 prime convergence (bl-18bf) through the REAL
//! `bl` binary — never the dev repo's own task list. The library unit tests
//! (`converge_tests`, `converge_debris_tests`) cover the module in isolation;
//! this proves the whole CLI → engine → converge/debris path on throwaway temp
//! repos: prime rewrites a retired `tracker` schedule to `bl-tracker` in one
//! landing commit (binding the new name, dropping the dangling old symlink) yet
//! leaves a live-bound `tracker` whole; the crash-debris report names its fixes
//! on stderr and suppresses the stealth.lock note once stealth is re-declared;
//! and a live op skips a still-`tracker` schedule with the non-fatal rename
//! notice while the op itself succeeds.

use assert_cmd::Command;
use balls::layout::{CloneDir, Xdg};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir (so the clone bundle lands there, not the real `$HOME`) and any
/// inherited plugin-chain env scrubbed (this suite may itself run inside a
/// close-hook plugin chain, the bl-spawning-test idiom).
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A real project on `main` with a seed commit, plus a founded landing (first
/// prime). Returns the project/home/state paths and the `CloneDir` bundle so a
/// test can read the landing + clone-root terminus directly.
fn primed(tmp: &Path) -> (PathBuf, PathBuf, PathBuf, CloneDir) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl(&project, &home, &state).arg("prime").assert().success();
    let clone = Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy())).clone_dir(&project);
    (project, home, state, clone)
}

/// The founded landing's `config/plugins/bin` binding store.
fn bin(landing: &Path) -> PathBuf {
    landing.join("config").join("plugins").join("bin")
}

/// Overwrite the founded landing's committed `config/plugins.toml` with `toml`
/// and commit it (clean tree), so the NEXT op sees the seeded schedule.
fn seed_schedule(landing: &Path, toml: &str) {
    std::fs::write(landing.join("config").join("plugins.toml"), toml).unwrap();
    git(landing, &["add", "-A"]);
    git(landing, &["-c", "user.name=t", "-c", "user.email=t@e", "commit", "-qm", "seed schedule"]);
}

/// Every commit subject on the landing's HEAD, newest first.
fn log_subjects(landing: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(landing)
        .args(["log", "--format=%s"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().lines().map(str::to_string).collect()
}

#[test]
fn prime_converges_a_retired_tracker_name_in_one_landing_commit() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state, clone) = primed(tmp.path());
    let landing = clone.landing();

    // A ≤0.5.3-era schedule: the retired `tracker` in the `[hooks]` arrays AND a
    // `[source]` key, with the now-dangling old symlink an old prime left behind
    // (and the founded `bl-tracker` binding removed, as a version-skewed checkout
    // written by the old binary would have it).
    seed_schedule(
        &landing,
        "[hooks]\n\"prime.pre\" = [\"tracker\"]\n\"create.post\" = [\"tracker\"]\n\n[source]\ntracker = \"cargo install balls\"\n",
    );
    std::fs::remove_file(bin(&landing).join("bl-tracker")).ok();
    symlink("/nonexistent/tracker", bin(&landing).join("tracker")).unwrap();

    bl(&project, &home, &state).arg("prime").assert().success();

    // Rewritten in place: no bare `"tracker"` array entry survives; the `[source]`
    // key is re-keyed to `bl-tracker` with its acquisition hint carried verbatim.
    let cfg = std::fs::read_to_string(landing.join("config").join("plugins.toml")).unwrap();
    assert!(!cfg.contains("[\"tracker\"]"), "hooks rewritten off the retired name: {cfg}");
    assert!(cfg.contains("[\"bl-tracker\"]"), "hooks now name the current plugin: {cfg}");
    assert!(cfg.contains("bl-tracker = \"cargo install balls\""), "[source] re-keyed, hint carried: {cfg}");

    // The seed's rule finished: current name bound to its sibling, dangling old
    // symlink dropped (the one deletion converge is allowed).
    assert!(bin(&landing).join("bl-tracker").symlink_metadata().is_ok(), "bl-tracker bound to its sibling");
    assert!(bin(&landing).join("tracker").symlink_metadata().is_err(), "dangling tracker symlink dropped");

    // Exactly ONE landing commit did it, named for the rename it applied.
    let subjects = log_subjects(&landing);
    let converge = subjects.iter().filter(|s| *s == "balls: converge tracker->bl-tracker").count();
    assert_eq!(converge, 1, "one converge commit, deterministically named: {subjects:?}");
}

#[test]
fn a_live_bound_tracker_name_is_left_untouched() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state, clone) = primed(tmp.path());
    let landing = clone.landing();

    // `tracker` is NOT reserved (only `bl-` is), so a third party may legitimately
    // ship a live-bound `tracker`. Point `bin/tracker` at a real binary so
    // `resolve_bin` is `Some` — the live-binding guard must leave it whole.
    let thirdparty = tmp.path().join("thirdparty-tracker");
    std::fs::write(&thirdparty, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&thirdparty, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    symlink(&thirdparty, bin(&landing).join("tracker")).unwrap();
    seed_schedule(&landing, "[hooks]\n\"prime.pre\" = [\"tracker\"]\n");
    let before = log_subjects(&landing).len();

    bl(&project, &home, &state).arg("prime").assert().success();

    // Schedule entry, symlink, and history all untouched: not our retired plugin.
    let cfg = std::fs::read_to_string(landing.join("config").join("plugins.toml")).unwrap();
    assert!(cfg.contains("[\"tracker\"]"), "live-bound name kept in the schedule: {cfg}");
    assert_eq!(std::fs::read_link(bin(&landing).join("tracker")).unwrap(), thirdparty, "binding untouched");
    assert_eq!(log_subjects(&landing).len(), before, "no converge commit for a live-bound name");
}

#[test]
fn debris_reports_orphan_changes_and_the_stealth_lock_until_stealth_is_declared() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state, clone) = primed(tmp.path());

    // Crash debris at the clone-bundle root (§1): an orphan `changes/<uuid>/`
    // worktree whose op teardown never ran, plus the retired `stealth.lock`. prime
    // REPORTS, deletes nothing (an orphan may hold uncommitted work).
    std::fs::create_dir_all(clone.change("dead-uuid")).unwrap();
    std::fs::write(clone.root().join("stealth.lock"), "").unwrap();
    let worktree_fix = format!("git worktree remove {}", clone.change("dead-uuid").display());

    bl(&project, &home, &state).arg("prime").assert().success().stderr(
        contains(worktree_fix.clone())
            .and(contains("stealth.lock is retired"))
            .and(contains("bl conf set task-remote none")),
    );

    // Both files remain (report-only), and the still-orphaned worktree keeps its
    // line — but declaring stealth via the modern sentinel suppresses the
    // stealth.lock note (the one silent-publish hazard, now re-declared).
    assert!(clone.root().join("stealth.lock").exists(), "report-only: stealth.lock is never deleted");
    bl(&project, &home, &state).args(["conf", "set", "task-remote", "none"]).assert().success();
    bl(&project, &home, &state)
        .arg("prime")
        .assert()
        .success()
        .stderr(contains("stealth.lock is retired").not().and(contains(worktree_fix)));
}

#[test]
fn a_live_op_skips_a_retired_tracker_with_a_nonfatal_notice_and_still_succeeds() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state, clone) = primed(tmp.path());
    let landing = clone.landing();

    // A schedule still naming the retired, UNBOUND `tracker` on `create.post` —
    // the version-skew case converge fixes on prime, observed here at DISPATCH on
    // a mutating verb (create does not converge). The stitch resolved no binding
    // for `tracker`, so dispatch skips it with the non-fatal rename notice.
    seed_schedule(&landing, "[hooks]\n\"create.post\" = [\"tracker\"]\n");

    let out = bl(&project, &home, &state)
        .args(["create", "A real task", "--as", "me"])
        .assert()
        .success()
        .stderr(contains("plugin tracker was renamed bl-tracker").and(contains("prime to resume")));

    // The op still produced its product: the minted id, alone on stdout (§9).
    let id = String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string();
    assert!(id.starts_with("bl-"), "the create op succeeded and printed its id: {id:?}");
}
