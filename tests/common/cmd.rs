//! `bl` subprocess invocation helpers for integration tests.
//!
//! Every command runs with HOME redirected to the per-thread test
//! HOME (via [`super::test_home_path`]) so concurrent integration
//! tests don't race on the shared XDG state tree. Split out of
//! `common/mod.rs` for the 300-line cap; re-exported there.

#![allow(dead_code)]

use assert_cmd::Command;
use std::path::{Path, PathBuf};

use super::test_home_path;

/// Path to the compiled `bl` binary.
pub fn bl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_bl"))
}

/// `bl` command bound to `cwd`, with the default test identity and
/// the per-thread test HOME wired in.
pub fn bl(cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd);
    c.env("BALLS_IDENTITY", "test-user");
    c.env("HOME", test_home_path());
    c
}

/// Like [`bl`] but with an explicit `BALLS_IDENTITY` value, for
/// multi-dev scenarios.
pub fn bl_as(cwd: &Path, identity: &str) -> Command {
    let mut c = Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd);
    c.env("BALLS_IDENTITY", identity);
    c.env("HOME", test_home_path());
    c
}

/// Run `bl create TITLE` and return the newly created ID (parsed from stdout).
pub fn create_task(cwd: &Path, title: &str) -> String {
    let out = bl(cwd).args(["create", title]).output().expect("bl create");
    assert!(
        out.status.success(),
        "bl create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run `bl create` with full options.
pub fn create_task_full(
    cwd: &Path,
    title: &str,
    priority: u8,
    deps: &[&str],
    tags: &[&str],
) -> String {
    let mut cmd = bl(cwd);
    cmd.arg("create").arg(title).arg("-p").arg(priority.to_string());
    for d in deps {
        cmd.arg("--dep").arg(d);
    }
    for t in tags {
        cmd.arg("--tag").arg(t);
    }
    let out = cmd.output().expect("bl create");
    assert!(
        out.status.success(),
        "bl create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run `bl init` in `cwd` and assert success.
pub fn init_in(cwd: &Path) {
    bl(cwd).arg("init").assert().success();
}

/// Run `bl doctor` and return stdout. Asserts exit 0 — doctor is
/// read-only and never fails the process; the verdict is in the text.
pub fn doctor(cwd: &Path) -> String {
    let out = bl(cwd).arg("doctor").output().expect("bl doctor");
    assert!(out.status.success(), "doctor must exit 0 (read-only)");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Run `bl show --json` for a (possibly archived) task and return the
/// parsed value. Asserts the command succeeded.
pub fn show_json(repo: &Path, id: &str) -> serde_json::Value {
    let out = bl(repo).args(["show", id, "--json"]).output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}
