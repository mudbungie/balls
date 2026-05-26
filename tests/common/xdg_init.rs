//! Shared helpers for the XDG `bl init` conformance test bundle.
//!
//! Phase 1B-2: the binary's `bl init` still routes through legacy
//! `Store::init` (sibling 1B-5 flips it). The conformance tests reach
//! the new code path directly via `balls::Store::init_xdg`, which
//! reads `HOME` from process env, so the calls serialize on a shared
//! `HOME` mutex; subsequent `bl` invocations go through `bl_xdg`
//! (subprocess env explicit, no race).

#![allow(dead_code)]

use balls::xdg_paths::XdgBases;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

pub fn bases(home: &Path) -> XdgBases {
    XdgBases::with(home, None, None, None)
}

/// Serializes the in-process `Store::init_xdg` calls across tests so
/// the per-test `HOME` override is observed cleanly. Subprocess `bl`
/// invocations set `HOME` on the child explicitly, so they don't need
/// this lock.
pub fn home_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holds the `HOME` mutex and the prior `HOME` value; restores on
/// drop so other tests in this binary see the original environment.
pub struct HomeOverride {
    _guard: MutexGuard<'static, ()>,
    prior: Option<std::ffi::OsString>,
}

impl HomeOverride {
    pub fn new(home: &Path) -> Self {
        let guard = home_lock();
        let prior = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home) };
        Self { _guard: guard, prior }
    }
}

impl Drop for HomeOverride {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}

/// Invoke `Store::init_xdg` with `HOME` pointed at the test tempdir,
/// then drop the override so subsequent calls run unguarded.
pub fn init_xdg(cwd: &Path, home: &Path, stealth: bool, tasks_dir: Option<String>) {
    let _h = HomeOverride::new(home);
    balls::Store::init_xdg(cwd, stealth, tasks_dir).expect("Store::init_xdg");
}

/// `bl` with a HOME pointed at the test tempdir, isolating XDG state.
pub fn bl_xdg(cwd: &Path, home: &Path) -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd)
        .env("HOME", home)
        .env("BALLS_IDENTITY", "test-user")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    c
}

/// Configure a fresh clone (no bl init yet) against `origin_url`, so
/// `Store::discover` will compute `<enc-origin>` from a real `origin`
/// remote. Returns the canonical absolute clone path.
pub fn fresh_clone_into(home: &Path, sub: &str, origin_url: &str, who: &str) -> PathBuf {
    let clone_root = home.join(sub);
    fs::create_dir_all(&clone_root).unwrap();
    let out = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(&clone_root)
        .output()
        .expect("git init");
    assert!(out.status.success());
    let email = format!("{who}@example.com");
    for (k, v) in [
        ("user.email", email.as_str()),
        ("user.name", who),
        ("commit.gpgsign", "false"),
    ] {
        let out = std::process::Command::new("git")
            .current_dir(&clone_root)
            .args(["config", k, v])
            .output()
            .expect("git config");
        assert!(out.status.success());
    }
    let out = std::process::Command::new("git")
        .current_dir(&clone_root)
        .args(["remote", "add", "origin", origin_url])
        .output()
        .expect("git remote add");
    assert!(out.status.success());
    fs::canonicalize(&clone_root).unwrap()
}

/// `git log main --format=%s` against `clone`, returning the commit
/// subjects in newest-first order. Empty when `main` has no commits.
pub fn main_log(clone: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .current_dir(clone)
        .args(["log", "main", "--format=%s"])
        .output()
        .expect("git log");
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}
