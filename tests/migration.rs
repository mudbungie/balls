//! bl-868d end-to-end: a fresh clone of a hub whose `origin/balls/tasks` is
//! still the PRE-greenfield LEGACY store (`.balls/tasks/*.json`, no `tasks/`).
//! Before the fix, `bl prime` ADOPTED that branch as the greenfield store (the
//! §12 adopt rule firing on a non-store tip), the delivery plugin aborted on
//! the missing `tasks/`, and every re-prime hit the same abort — the §12
//! no-op-converge property was lost and the §16 runbook's step 2 ("prime
//! founds substrate + an empty store; import fills it") was impossible on any
//! shared repo. Now the tracker QUARANTINES a no-`tasks/` tip (warns, adopts
//! nothing, never rewrites it), so prime founds fresh and converges, and the
//! whole §16 sequence — prime → preview → import → cutover — runs as written.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Git;
use tempfile::TempDir;

/// Run a setup git command, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    assert!(Git::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success());
}

/// `cwd`'s tip of `rev`, trimmed.
fn tip(cwd: &Path, rev: &str) -> String {
    let out = Git::new("git").arg("-C").arg(cwd).args(["rev-parse", rev]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The freshly-built `bl` rooted in `project`, XDG-isolated under the tempdir.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// A bare hub carrying `main` plus a LEGACY `balls/tasks` (task JSON under
/// `.balls/tasks/`, NO `tasks/`), and a fresh clone of it — the §16 shared-repo
/// migration starting point.
fn legacy_hub_and_clone(tmp: &Path) -> (PathBuf, PathBuf) {
    let hub = tmp.join("hub.git");
    git(tmp, &["init", "--bare", "-q", "-b", "main", &hub.to_string_lossy()]);
    let seed = tmp.join("seed");
    git(tmp, &["clone", "-q", &hub.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "t"]);
    git(&seed, &["config", "user.email", "t@e"]);
    fs::write(seed.join("README.md"), "hi\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "init"]);
    git(&seed, &["push", "-q", "origin", "main"]);
    git(&seed, &["checkout", "-q", "--orphan", "balls/tasks"]);
    git(&seed, &["rm", "-rq", "--cached", "."]);
    fs::remove_file(seed.join("README.md")).unwrap();
    fs::create_dir_all(seed.join(".balls/tasks")).unwrap();
    fs::write(
        seed.join(".balls/tasks/bl-aaaa.json"),
        r#"{"id":"bl-aaaa","title":"legacy task","status":"open","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","description":"carried over"}"#,
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "legacy store"]);
    git(&seed, &["push", "-q", "origin", "balls/tasks"]);
    let clone = tmp.join("clone");
    git(tmp, &["clone", "-q", &hub.to_string_lossy(), &clone.to_string_lossy()]);
    (hub, clone)
}

#[test]
fn prime_on_a_legacy_carrying_hub_founds_fresh_converges_and_imports() {
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let (hub, clone) = legacy_hub_and_clone(tmp.path());
    let legacy_tip = tip(&hub, "balls/tasks");

    // §16 runbook step 2 on a fresh clone: prime succeeds, warns about the
    // un-cut-over legacy ref, and founds a FRESH greenfield store instead of
    // adopting the legacy branch (the bl-868d wedge).
    bl(&clone, &home, &state)
        .arg("prime")
        .assert()
        .success()
        .stderr(contains("not a greenfield store"));
    // §12 no-op-converge: a re-prime succeeds too (it used to re-abort).
    bl(&clone, &home, &state).arg("prime").assert().success();
    // The hub's legacy ref was never rewritten — cutover is the operator's
    // explicit force-push (runbook step 5), not an implicit side effect.
    assert_eq!(tip(&hub, "balls/tasks"), legacy_tip);

    // Steps 3+4: the preview reads the legacy store from the clone's
    // remote-tracking ref, and the cutover button imports it into the fresh
    // store — the per-op publish skips the un-cut-over ref instead of E5ing.
    bl(&clone, &home, &state)
        .args(["list", "--legacy=origin/balls/tasks"])
        .assert()
        .success()
        .stdout(contains("bl-aaaa"));
    bl(&clone, &home, &state)
        .args(["import", "--legacy=origin/balls/tasks", "--as", "mig"])
        .assert()
        .success();
    bl(&clone, &home, &state).arg("list").assert().success().stdout(contains("legacy task"));
}
