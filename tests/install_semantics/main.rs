//! End-to-end `bl install` semantics (bl-0df2): the copy-SHAPE dispatch (§6) and
//! the bind half, driven through the freshly-built `bl` on throwaway primed
//! clones — never the dev repo's own store.
//!
//! `install` copies a committed path between two LOCAL balls branches, and the
//! path's shape alone decides the merge: a FOLDER mirrors (deletions propagate),
//! a FILE/glob unions (additive, source-wins, siblings untouched). `bin/` never
//! travels; a `--bin` with neither an explicit `<path>` nor `--from` is BIND-ONLY
//! (bl-cfe3) — it copies nothing, it just binds. The direction/refusal legs live
//! in the [`directions`] sibling module (split for the 300-line cap).
//!
//! Unix-only: the bind-only test ships a `/bin/sh` fake plugin, and the whole
//! target already leans on POSIX perms for it (the `tests/enrollment.rs` shape).

#![cfg(unix)]

mod binding;
mod directions;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir so the clone bundle never touches the real `$HOME`. `XDG_CONFIG_HOME`
/// removed so no host config leaks in. Any inherited plugin-chain env is scrubbed
/// so a `bl`-in-a-hook parent can't nest us (the parallel-test note).
pub(crate) fn bl(project: &Path, home: &Path, state: &Path) -> Command {
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
pub(crate) fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// `git -C <cwd> add -A && commit` with a pinned identity (HOME is a fresh
/// tempdir, so there is no ambient `user.name`). Advances the branch the worktree
/// sits on — how a test seeds an "extra" destination file for the mirror to prune.
pub(crate) fn commit(cwd: &Path, msg: &str) {
    git(cwd, &["add", "-A"]);
    git(cwd, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", msg]);
}

/// The clone bundle (landing/store paths) for an invocation at `project`.
pub(crate) fn clone_at(state: &Path, project: &Path) -> balls::layout::CloneDir {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy())).clone_dir(project)
}

/// A primed throwaway project (no git repo of its own needed — install is purely
/// local to the clone bundle). Returns `(project, home, state)`.
pub(crate) fn primed(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    fs::create_dir_all(&project).unwrap();
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// A real, bindable plugin: answers `<bin> protocol` with `ops`, drains stdin,
/// exits 0. Enough for `install`'s `resolve_and_bind` validation to accept it.
fn fake_plugin(dir: &Path, name: &str, ops: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    let body =
        format!("#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":{ops}}}'; exit 0; fi\ncat >/dev/null\nexit 0\n");
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn a_folder_mirror_publishes_config_to_the_store_then_prunes_a_dropped_file() {
    // §6 shape=folder ⇒ MIRROR: the `--to balls/tasks` publish direction populates
    // the store's `config/`, and a later re-publish DELETES a destination file the
    // source lacks (deletions propagate — the close-resurrection cure). The
    // `tasks/` sibling is never touched (install writes only under `<path>`).
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());
    let store = clone_at(&state, &project).store();

    // Publish landing→store: mirror lands both config files onto the store tip.
    bl(&project, &home, &state)
        .args(["install", "config", "--from", "balls/config", "--to", "balls/tasks"])
        .assert()
        .success()
        .stdout(contains("install: 2 added / 0 deleted"));
    assert!(store.join("config/balls.toml").is_file(), "store checkout reflects the publish");
    assert!(store.join("tasks/.gitkeep").is_file(), "the tasks/ sibling is untouched");

    // Seed an extra file the source lacks, committed onto the store branch tip.
    fs::write(store.join("config/extra.txt"), "junk\n").unwrap();
    commit(&store, "add extra");

    // Re-publish: the folder mirror removes it (1 deleted); siblings still stand.
    bl(&project, &home, &state)
        .args(["install", "config", "--from", "balls/config", "--to", "balls/tasks"])
        .assert()
        .success()
        .stdout(contains("/ 1 deleted"));
    assert!(!store.join("config/extra.txt").exists(), "mirror deletes the dropped file");
    assert!(store.join("config/balls.toml").is_file(), "the mirrored files remain");
    assert!(store.join("tasks/.gitkeep").is_file(), "the sibling is still untouched");
}

#[test]
fn a_single_file_and_a_glob_union_leave_unrelated_destination_files_untouched() {
    // §6 shape=file/glob ⇒ UNION: additive, source-wins on overlap, the
    // destination's OTHER files survive. A `config/keep.txt` the source never
    // carries is present after both a single-file and a `config/*` glob install.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());
    let store = clone_at(&state, &project).store();

    // An unrelated destination file, committed onto the store tip.
    fs::create_dir_all(store.join("config")).unwrap();
    fs::write(store.join("config/keep.txt"), "keep\n").unwrap();
    commit(&store, "seed keep");

    // Single-file union: exactly one file copied, `keep.txt` untouched.
    bl(&project, &home, &state)
        .args(["install", "config/plugins.toml", "--from", "balls/config", "--to", "balls/tasks"])
        .assert()
        .success()
        .stdout(contains("install: 1 added / 0 deleted"));
    assert!(store.join("config/plugins.toml").is_file(), "the unioned file landed");
    assert!(store.join("config/keep.txt").is_file(), "a single-file union spares unrelated files");

    // Glob union: both `config/*` files copied, still 0 deleted, `keep.txt` stays.
    bl(&project, &home, &state)
        .args(["install", "config/*", "--from", "balls/config", "--to", "balls/tasks"])
        .assert()
        .success()
        .stdout(contains("install: 2 added / 0 deleted"));
    assert!(store.join("config/balls.toml").is_file(), "the glob unioned every source file");
    assert!(store.join("config/keep.txt").is_file(), "a glob union spares unrelated files too");
}

#[test]
fn a_bin_only_install_copies_nothing_but_binds_the_referenced_plugin() {
    // bl-cfe3: `--bin <name>=<path>` with neither an explicit `<path>` nor a
    // `--from` is BIND-ONLY — it seals no copy (0 added / 0 deleted) yet the
    // referenced plugin ends up bound under `config/plugins/bin/`.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());
    let bin = fake_plugin(&tmp.path().join("bins"), "ghost", r#"["list"]"#);
    let landing = clone_at(&state, &project).landing();
    let link = landing.join("config/plugins/bin/ghost");

    // A referenced name is a precondition to bind (unreferenced names never bind
    // silently); a harmless read hook supplies the reference.
    bl(&project, &home, &state).args(["conf", "append", "list", "ghost"]).assert().success();
    assert!(!link.exists(), "no binding before the bind-only install");

    bl(&project, &home, &state)
        .args(["install", "--bin", &format!("ghost={}", bin.display())])
        .assert()
        .success()
        .stdout(contains("install: 0 added / 0 deleted"));
    assert!(link.exists(), "bind-only copies nothing but DOES bind the plugin");
}

#[test]
fn install_as_stamps_the_actor_onto_the_sealed_commit() {
    // `--as ID` rides the §5 checkout message: the sealed commit carries the
    // actor trailer, the observable proof the identity flowed through.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());
    let landing = clone_at(&state, &project).landing();

    bl(&project, &home, &state)
        .args(["install", "config", "--from", "balls/config", "--to", "balls/tasks", "--as", "alice"])
        .assert()
        .success();

    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&landing)
        .args(["log", "-1", "--format=%B", "balls/tasks"])
        .output()
        .unwrap();
    let body = String::from_utf8(out.stdout).unwrap();
    assert!(body.contains("bl-actor: alice"), "install --as stamps the actor trailer: {body}");
}
