//! Shared helpers for `bl migrate` conformance tests. Standing up a
//! pre-XDG legacy clone (the only real-world starting point for
//! migration) is the load-bearing test fixture; pulling it into a
//! sibling module keeps the per-test files focused on assertions.

#![allow(dead_code)]

use super::{git, git_ok, new_bare_remote, push, Repo};
use balls::xdg_paths::XdgBases;
use std::fs;
use std::path::{Path, PathBuf};

/// XDG bases rooted under `home`, with HOME-derived defaults for the
/// three XDG vars. Matches what `XdgBases::from_env` builds for a
/// subprocess inheriting a custom HOME.
pub fn bases(home: &Path) -> XdgBases {
    XdgBases::with(home, None, None, None)
}

/// `<nested-clone-path>` for an absolute directory.
pub fn nested(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    PathBuf::from(s.strip_prefix('/').unwrap_or(&s))
}

/// Set up a fresh bare remote and a legacy-layout clone of it. The
/// returned clone is pre-XDG: `.balls/config.json` on `main`, a
/// `.balls/state-repo/` runtime checkout, and balls entries in
/// `.gitignore`. Tuple: (remote-repo-tempdir, canonicalized clone path,
/// origin URL string).
pub fn legacy_clone(home: &Path, sub: &str) -> (Repo, PathBuf, String) {
    let remote = new_bare_remote();
    let url = remote.path().to_string_lossy().into_owned();
    let clone_root = home.join(sub);
    fs::create_dir_all(&clone_root).unwrap();
    let out = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(remote.path())
        .arg(&clone_root)
        .output()
        .expect("git clone");
    assert!(
        out.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for (k, v) in [
        ("user.email", "alice@example.com"),
        ("user.name", "alice"),
        ("commit.gpgsign", "false"),
        ("init.defaultBranch", "main"),
    ] {
        git(&clone_root, &["config", k, v]);
    }
    if !git_ok(&clone_root, &["rev-parse", "HEAD"]) {
        fs::write(clone_root.join("README"), "seed\n").unwrap();
        git(&clone_root, &["add", "README"]);
        git(&clone_root, &["commit", "-m", "seed", "--no-verify"]);
    }
    bl_xdg(&clone_root, home).arg("init").assert().success();
    push(&clone_root);
    let canonical = fs::canonicalize(&clone_root).unwrap();
    (remote, canonical, url)
}

/// `bl` configured with HOME pointed at `home` — every XDG path
/// resolves under the tempdir, never the real user's `~/.local/`.
pub fn bl_xdg(cwd: &Path, home: &Path) -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd)
        .env("HOME", home)
        .env("BALLS_IDENTITY", "test-user");
    c
}

/// Read `origin.url` from the clone's git config.
pub fn origin_url_of(clone: &Path) -> String {
    let out = std::process::Command::new("git")
        .current_dir(clone)
        .args(["remote", "get-url", "origin"])
        .output()
        .expect("git remote get-url");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
