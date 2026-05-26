//! Shared helpers for `bl migrate` conformance tests. Standing up a
//! pre-XDG legacy clone (the only real-world starting point for
//! migration) is the load-bearing test fixture; pulling it into a
//! sibling module keeps the per-test files focused on assertions.

#![allow(dead_code)]

use super::{git, git_ok, new_bare_remote, push, Repo};
use balls::config::Config;
use balls::state_repo;
use balls::tracker_address;
use balls::xdg_paths::XdgBases;
use std::fs;
use std::path::{Path, PathBuf};

/// The legacy `.gitignore` entries `bl init` (Phase 1A) wrote at the
/// clone root. Frozen here so the fixture survives `cmd_init` switching
/// to `Store::init_xdg` — see bl-a684. Mirrors
/// `runtime_paths::gitignore_paths(false)` at the time bl-e802 began.
const LEGACY_GITIGNORE: &[&str] = &[
    ".balls/local",
    ".balls/tasks",
    ".balls/project.json",
    ".balls/state-repo",
    ".balls/plugins",
    ".balls/code-refs",
    ".balls-worktrees",
];

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
///
/// The legacy post-state is hand-scaffolded (not produced by
/// `bl init`) so this fixture stays stable across the Phase 1B
/// `cmd_init`-to-`Store::init_xdg` flip — see bl-a684. The shape
/// mirrors SPEC-clone-layout §11's pre-XDG layout: `.balls/config.json`
/// on the code branch, the unified state checkout at
/// `.balls/state-repo/`, runtime paths in `.gitignore`, and a single
/// `balls: initialize` commit on `main`.
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

    // Pre-XDG scaffolding under `.balls/` and the repo-tracked
    // `config.json` marker. The state-checkout symlinks and the
    // `balls/tasks` orphan are materialized by `state_repo::ensure`
    // below.
    let balls = clone_root.join(".balls");
    fs::create_dir_all(balls.join("plugins")).unwrap();
    for d in ["local/claims", "local/lock", "local/plugins"] {
        fs::create_dir_all(balls.join(d)).unwrap();
    }
    let cfg_path = balls.join("config.json");
    Config::default().save(&cfg_path).unwrap();

    // Write `.gitignore` *before* `state_repo::ensure` so its embedded
    // `legacy_plugin_migrate::run(root)` finds nothing missing and
    // exits without making its own `balls: migrate plugins ...` commit
    // on main — the legacy contract has exactly one `balls: initialize`
    // commit.
    write_legacy_gitignore(&clone_root);

    let cfg = Config::load(&cfg_path).unwrap();
    let addr = tracker_address::resolve(&clone_root, &cfg);
    state_repo::ensure(&clone_root, &addr).unwrap();

    git(&clone_root, &["add", ".gitignore", ".balls/config.json"]);
    git(&clone_root, &["commit", "-m", "balls: initialize", "--no-verify"]);

    push(&clone_root);
    let canonical = fs::canonicalize(&clone_root).unwrap();
    (remote, canonical, url)
}

/// Append every `LEGACY_GITIGNORE` entry missing from `<root>/.gitignore`,
/// one per line. Idempotent — mirrors `gitignore::ensure_main_gitignore`
/// without coupling tests to the private `runtime_paths` table.
fn write_legacy_gitignore(root: &Path) {
    let path = root.join(".gitignore");
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    for entry in LEGACY_GITIGNORE {
        if content.lines().any(|l| l.trim() == *entry) {
            continue;
        }
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(entry);
        content.push('\n');
    }
    fs::write(&path, content).unwrap();
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
