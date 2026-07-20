//! `bl conf` writes taking FUNCTIONAL effect, not just file/provenance effect
//! (bl-abf0). The scalar conf suite (`conf_scalar/`) pins where each write LANDS
//! and what the read reports; this file proves the writes change BEHAVIOR:
//! - `set task-remote <B>` re-points live traffic — the next mutating op founds
//!   B's `balls/tasks` and A stops advancing (the §12 ladder actually re-resolves);
//! - `remove <op>.<phase> <name>` stops dispatch — a bound marker plugin fires
//!   while wired and goes silent once unwired (the schedule edit halts the call);
//! - the legacy global XDG `remote` tier is OPERATIVE, not merely displayed — with
//!   no origin/binding/sentinel, prime+create push through it (its ref moves).
//!
//! Own ~60-line harness (per the no-shared-harness rule — each `tests/*.rs` is its
//! own crate), driving the freshly-built `bl` + shipped `bl-tracker` against
//! throwaway repos in a `TempDir`, HOME/XDG pinned into the tempdir.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// `bl` rooted in `project`, HOME/`$XDG_STATE_HOME` pinned into the tempdir and
/// `XDG_CONFIG_HOME` removed (so the global tier is `$HOME/.config/balls`), plus
/// the inherited plugin-chain env scrubbed (this test runs inside a close hook).
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

/// The clone bundle (landing/store/binding) bl resolves for an invocation.
fn clone_dir(home: &Path, state: &Path, project: &Path) -> balls::layout::CloneDir {
    balls::layout::Xdg::with(home, None, Some(&state.to_string_lossy())).clone_dir(project)
}

/// A fresh git project on `main` with a seed commit and NO `origin` (stealth).
fn stealth_project(dir: &Path) -> PathBuf {
    let project = dir.join("p");
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "t"]);
    git(&project, &["config", "user.email", "t@t"]);
    fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    project
}

/// An EMPTY bare repo at `dir/<name>.git` — a reachable founding target.
fn empty_bare(dir: &Path, name: &str) -> PathBuf {
    let bare = dir.join(format!("{name}.git"));
    git(dir, &["init", "--bare", "-q", "-b", "main", &bare.to_string_lossy()]);
    bare
}

/// `balls/tasks` tip on `bare`, or `None` when the branch is absent.
fn tasks_tip(bare: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(bare)
        .args(["rev-parse", "--verify", "-q", "balls/tasks"])
        .output()
        .unwrap();
    out.status.success().then(|| String::from_utf8(out.stdout).unwrap().trim().to_string())
}

/// The self-describing HEAD every fake plugin answers to `<bin> protocol` (so
/// dispatch validation is happy); copied from `tests/protocol_edges/harness.rs`.
const HEAD: &str = "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{\"protocol\":[1],\"ops\":[\"create\",\"list\",\"show\"]}'; exit 0; fi\n";

/// Write an executable fake plugin under `dir/<name>` whose non-`protocol` body is
/// `body`, then bind it as the landing's local `config/plugins/bin/<name>` symlink.
/// The write-elsewhere-then-symlink two-step is the ETXTBSY-safe shape the harness
/// uses (the exec target is the stable symlink, not the file just written).
fn bind_plugin(clone: &balls::layout::CloneDir, dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    let mut script = String::from(HEAD);
    script.push_str(body);
    script.push('\n');
    fs::write(&path, &script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let bin = clone.landing().join("config").join("plugins").join("bin");
    fs::create_dir_all(&bin).unwrap();
    let link = bin.join(name);
    let _ = fs::remove_file(&link);
    std::os::unix::fs::symlink(&path, &link).unwrap();
}

#[test]
fn set_task_remote_redirects_live_traffic_to_the_new_remote() {
    // The point conf_scalar can't make: the binding write actually re-resolves the
    // §12 ladder for the NEXT op. Prime against origin A, publish a task (founds A),
    // then `conf set task-remote B` — a further create founds B and A stops moving.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());
    let a = empty_bare(tmp.path(), "a");
    let b = empty_bare(tmp.path(), "b");
    git(&project, &["remote", "add", "origin", &a.to_string_lossy()]);

    bl(&project, &home, &state).arg("prime").assert().success();
    // First create resolves task-remote via origin ⇒ founds A's balls/tasks.
    bl(&project, &home, &state).args(["create", "On A", "--as", "me"]).assert().success();
    let a_tip1 = tasks_tip(&a).expect("origin A founded by the first create");
    assert!(tasks_tip(&b).is_none(), "B untouched before the rebind");

    // Re-point: the binding remote now outranks origin.
    bl(&project, &home, &state).args(["conf", "set", "task-remote", &b.to_string_lossy()]).assert().success();
    bl(&project, &home, &state)
        .args(["conf", "task-remote"])
        .assert()
        .success()
        .stdout(contains(b.to_string_lossy().to_string()))
        .stderr(contains("from binding"));

    // The next mutating op must land on B, and A must NOT advance.
    bl(&project, &home, &state).args(["create", "On B", "--as", "me"]).assert().success();
    let b_tip = tasks_tip(&b).expect("B founded by the post-rebind create");
    assert_eq!(tasks_tip(&a).unwrap(), a_tip1, "A must stop receiving after the rebind");
    assert_ne!(b_tip, a_tip1, "B carries the new task A never saw");

    // A plain `sync` now pulls from B, not A — the ladder change is durable.
    bl(&project, &home, &state).args(["sync"]).assert().success();
    assert_eq!(tasks_tip(&a).unwrap(), a_tip1, "sync must not touch the old remote either");
}

#[test]
fn removing_a_hook_stops_its_dispatch() {
    // conf writing the [hooks] schedule is only meaningful if the change alters
    // dispatch. Wire a marker plugin onto create.post, prove it stamps, `conf
    // remove` it, and prove the SAME op no longer stamps — the edit halts the call.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());
    let plugins = tmp.path().join("plugins");
    fs::create_dir_all(&plugins).unwrap();
    let marker = tmp.path().join("marker");

    bl(&project, &home, &state).arg("prime").assert().success();
    let clone = clone_dir(&home, &state, &project);
    // Stamp one line per forward invocation into the marker file.
    bind_plugin(&clone, &plugins, "bl-marker", &format!("echo stamp >> {}\nexit 0", marker.display()));

    // Prepend (run before the tracker) so a stamp lands even on a stealth create.
    bl(&project, &home, &state).args(["conf", "prepend", "create.post", "bl-marker"]).assert().success();
    bl(&project, &home, &state).args(["create", "wired", "--as", "me"]).assert().success();
    let stamped = fs::read_to_string(&marker).expect("the wired plugin stamped once");
    assert_eq!(stamped.lines().count(), 1, "exactly one stamp while wired: {stamped:?}");

    // Unwire and re-run the identical op: no NEW stamp may appear.
    bl(&project, &home, &state).args(["conf", "remove", "create.post", "bl-marker"]).assert().success();
    bl(&project, &home, &state).args(["create", "unwired", "--as", "me"]).assert().success();
    let after = fs::read_to_string(&marker).unwrap();
    assert_eq!(after.lines().count(), 1, "the remove must stop dispatch — no second stamp: {after:?}");
}

#[test]
fn the_legacy_global_xdg_remote_tier_is_operative_not_just_displayed() {
    // The §12 ladder's legacy tier: a global `remote` in $HOME/.config/balls with
    // NO per-clone binding, NO landing sentinel, NO git origin. If it only DISPLAYED
    // (conf_scalar proves the read), the store would found nowhere. Prove traffic:
    // prime+create push balls/tasks through the XDG-configured remote (its ref moves).
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path()); // no origin
    let c = empty_bare(tmp.path(), "c");
    fs::create_dir_all(home.join(".config").join("balls")).unwrap();
    fs::write(home.join(".config").join("balls").join("config.toml"), format!("remote = \"{}\"\n", c.display())).unwrap();

    bl(&project, &home, &state).arg("prime").assert().success();
    // The read reports the tier (the file effect conf_scalar already pins)...
    bl(&project, &home, &state)
        .args(["conf", "task-remote"])
        .assert()
        .success()
        .stdout(contains(c.to_string_lossy().to_string()))
        .stderr(contains("xdg (global)"));

    // ...and it must be FUNCTIONAL: with nothing else in the ladder, the store
    // founds and pushes through it.
    bl(&project, &home, &state).args(["create", "Via the global remote", "--as", "me"]).assert().success();
    assert!(tasks_tip(&c).is_some(), "the global XDG remote must actually receive balls/tasks, not merely display");
}
