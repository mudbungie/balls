//! End-to-end: the install-chain wedge and its conf escape (bl-4edc).
//!
//! Binding runs AFTER the install copy seals (bl-4c45, intended): the schedule
//! is committed text, binding is this box's local resolution of it. The wedge
//! shape: a schedule wiring a dangling plugin into `install`'s OWN pre chain
//! aborts every retry at dispatch, before bind can run — install cannot repair
//! install. The escape-hatch property under test: `bl conf` runs no plugins,
//! so the schedule is always editable in-band. These tests hold that property
//! with the real binaries, not by luck.

#![cfg(unix)]

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir so the clone bundle never touches the real `$HOME` (the
/// `tests/dispatch.rs` harness). Stealth box: the tracker no-ops on fetch.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// A real, bindable plugin: answers `<bin> protocol` with `ops`, drains stdin,
/// and stamps `marker` so a test can prove it actually dispatched.
fn fake_plugin(dir: &Path, name: &str, ops: &str, marker: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":{ops}}}'; exit 0; fi\ncat >/dev/null\nprintf x >> {}\nexit 0\n",
        marker.display()
    );
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// One primed throwaway project; returns (tempdir, project, home, state).
fn primed() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    fs::create_dir_all(&project).unwrap();
    bl(&project, &home, &state).arg("prime").assert().success();
    (tmp, project, home, state)
}

/// The landing→landing self-copy from `tests/dispatch.rs` — the cheapest real
/// install (no-op seal), used here to drive the install chain end to end.
const SELF_COPY: [&str; 6] = ["install", "config", "--from", "balls/config", "--to", "balls/config"];

/// The self-copy argv plus an explicit `--bin ghost=<bin>` candidate.
fn self_copy_with_bin(bin: &Path) -> Vec<String> {
    SELF_COPY.iter().map(ToString::to_string).chain(["--bin".into(), format!("ghost={}", bin.display())]).collect()
}

#[test]
fn a_dangling_plugin_in_installs_own_chain_recovers_via_the_conf_escape() {
    // The general wedge: the adopted schedule references the plugin BEYOND
    // install's own chain too (here claim.post), so the documented recipe —
    // conf remove → install --bin → conf prepend — converges directly.
    let (tmp, project, home, state) = primed();
    let marker = tmp.path().join("ghost-ran");
    let bin = fake_plugin(&tmp.path().join("bins"), "ghost", r#"["install","claim"]"#, &marker);

    bl(&project, &home, &state).args(["conf", "append", "install.pre", "ghost"]).assert().success();
    bl(&project, &home, &state).args(["conf", "append", "claim.post", "ghost"]).assert().success();

    // Wedged: dispatch of install's own pre chain precedes bind, so even a
    // retry carrying --bin never reaches the binder.
    bl(&project, &home, &state).args(SELF_COPY).assert().failure()
        .stderr(contains("plugin ghost referenced but bin/ghost missing"));
    let with_bin = self_copy_with_bin(&bin);
    bl(&project, &home, &state).args(&with_bin).assert().failure()
        .stderr(contains("plugin ghost referenced but bin/ghost missing"));

    // The escape: conf runs no plugins, so the schedule is always editable.
    bl(&project, &home, &state).args(["conf", "remove", "install.pre", "ghost"]).assert().success();
    bl(&project, &home, &state).args(&with_bin).assert().success();
    bl(&project, &home, &state).args(["conf", "prepend", "install.pre", "ghost"]).assert().success();

    // Healthy: ghost resolves and actually dispatches in install's pre chain.
    bl(&project, &home, &state).args(SELF_COPY).assert().success();
    assert!(marker.is_file(), "ghost never dispatched after recovery");
}

#[test]
fn a_plugin_referenced_only_in_installs_chain_binds_via_a_temporary_reference() {
    // The corner: install.pre is the plugin's ONLY reference. After the conf
    // remove, --bin is refused (unreferenced names are never silently bound),
    // so the recipe needs one more move — a temporary reference on a harmless
    // read hook, dropped after the bind.
    let (tmp, project, home, state) = primed();
    let marker = tmp.path().join("ghost-ran");
    let bin = fake_plugin(&tmp.path().join("bins"), "ghost", r#"["install","list"]"#, &marker);

    bl(&project, &home, &state).args(["conf", "append", "install.pre", "ghost"]).assert().success();
    bl(&project, &home, &state).args(SELF_COPY).assert().failure()
        .stderr(contains("plugin ghost referenced but bin/ghost missing"));

    bl(&project, &home, &state).args(["conf", "remove", "install.pre", "ghost"]).assert().success();
    let with_bin = self_copy_with_bin(&bin);
    bl(&project, &home, &state).args(&with_bin).assert().failure()
        .stderr(contains("--bin ghost: the landed schedule does not reference that plugin"));

    // Temporary reference: a read-op hook (a failed read dispatch is non-fatal
    // by design, so even mid-recipe it can't block anything).
    bl(&project, &home, &state).args(["conf", "append", "list", "ghost"]).assert().success();
    bl(&project, &home, &state).args(&with_bin).assert().success();
    bl(&project, &home, &state).args(["conf", "remove", "list", "ghost"]).assert().success();
    bl(&project, &home, &state).args(["conf", "prepend", "install.pre", "ghost"]).assert().success();

    bl(&project, &home, &state).args(SELF_COPY).assert().success();
    assert!(marker.is_file(), "ghost never dispatched after recovery");
}
