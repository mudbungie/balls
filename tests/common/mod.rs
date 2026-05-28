//! Shared helpers for integration tests.
//!
//! Tests spin up real git repositories in temp dirs and run the `bl` binary
//! as a subprocess. A typical multi-dev scenario uses a bare "remote" and
//! two cloned working repos as Dev A and Dev B.

#![allow(dead_code, unused_imports)]

mod cmd;
mod config_seed;
pub mod forge;
pub mod migrate;
pub mod multidev;
pub mod native_plugin;
mod paths;
pub mod plugin;
pub mod tracker;
pub mod xdg_init;
mod xdg_repo;

pub use cmd::{
    bl, bl_as, bl_bin, create_task, create_task_full, doctor, init_in,
    set_default_target_branch, show_json,
};
pub use config_seed::{edit_and_commit_repo_config, seed_config, set_project_plugins};
pub use migrate::legacy_clone;
pub use paths::{
    cache_dir, cache_last_fetch, claims_dir, config_path, discover_state_repo, discover_tasks_dir,
    local_dir, lock_dir, per_clone_paths, plugin_config_root, plugins_auth_dir,
    project_config_path, test_xdg_bases, worktree_path, worktrees_dir,
};
pub use xdg_repo::{new_xdg_repo, XdgRepo};

use balls::git::clean_git_command;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A pair of (temp dir, repo root path). The TempDir must be kept alive to
/// prevent cleanup. Optionally keeps a sibling bare remote alive so
/// `Repo::path()` has a reachable `origin` — XDG `bl init` requires one.
pub struct Repo {
    pub dir: TempDir,
    /// A bare remote pointed at by `origin`. Kept alive for the
    /// lifetime of the `Repo` so the tracker URL stays resolvable.
    /// `None` for bare/no-origin fixtures.
    #[allow(dead_code)]
    pub origin_remote: Option<TempDir>,
}

impl Repo {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

// Per-thread HOME tempdir for XDG isolation. Each libtest worker gets
// its own home, so concurrent integration tests do not race on the
// XDG state tree. Repos within one test share this tempdir (they run
// on the same thread), which mirrors real bilateral mobility — two
// clones of one origin on one machine share `trackers/<enc-origin>/`.
// Allocated lazily on first `bl()` (or `bl_as()`) call.
thread_local! {
    static TEST_HOME: std::cell::RefCell<Option<TempDir>> =
        const { std::cell::RefCell::new(None) };
}

/// Path to this thread's HOME tempdir, materializing it on first use.
///
/// Seeds a minimal `.gitconfig` with a test identity so any nested
/// git checkout `bl` creates (state-repo, tracker checkout) can
/// commit without falling back to the developer's real ~/.gitconfig
/// (which the HOME redirect has just hidden).
pub fn test_home_path() -> PathBuf {
    TEST_HOME.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            let dir = tempfile::Builder::new()
                .prefix("balls-it-home-")
                .tempdir()
                .expect("home tempdir");
            std::fs::write(
                dir.path().join(".gitconfig"),
                "[user]\n\tname = Test Dev\n\temail = dev@example.com\n[commit]\n\tgpgsign = false\n",
            )
            .expect("seed .gitconfig");
            *opt = Some(dir);
        }
        opt.as_ref().unwrap().path().to_path_buf()
    })
}

pub fn tmp() -> TempDir {
    tempfile::Builder::new()
        .prefix("balls-it-")
        .tempdir()
        .expect("tempdir")
}

pub fn git(cwd: &Path, args: &[&str]) -> String {
    let out = clean_git_command(cwd).args(args).output().expect("git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub fn git_ok(cwd: &Path, args: &[&str]) -> bool {
    clean_git_command(cwd)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Initialize a fresh git repo at a temp path with a configured user and
/// initial branch "main". Wires a bare `origin` remote at a sibling
/// tempdir — XDG `bl init` requires an origin URL to derive the tracker
/// address — and seeds a commit on `main` so HEAD is valid (XDG init no
/// longer writes the legacy "balls: initialize" seed). The remote stays
/// alive on the returned `Repo`.
pub fn new_repo() -> Repo {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "dev@example.com"]);
    git(dir.path(), &["config", "user.name", "Test Dev"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    let remote = tmp();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        dir.path(),
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    git(dir.path(), &["commit", "--allow-empty", "-qm", "seed", "--no-verify"]);
    git(dir.path(), &["push", "-q", "origin", "main"]);
    Repo { dir, origin_remote: Some(remote) }
}

/// Create a bare repository at a temp path to act as a remote.
pub fn new_bare_remote() -> Repo {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "--bare", "-b", "main"]);
    Repo { dir, origin_remote: None }
}

/// Like [`new_repo`] but with no `origin` remote wired. Tests that
/// specifically exercise the "no origin" provenance path (`repo` field
/// is null at create) use this — the default `new_repo` always wires
/// origin so XDG `bl init` has a tracker address. Callers must use
/// `--stealth` or add origin themselves before running `bl init`.
pub fn new_repo_no_origin() -> Repo {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "dev@example.com"]);
    git(dir.path(), &["config", "user.name", "Test Dev"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    git(dir.path(), &["commit", "--allow-empty", "-qm", "seed", "--no-verify"]);
    Repo { dir, origin_remote: None }
}

/// Clone a bare remote into a fresh temp dir as a developer clone.
/// If the remote is empty (no commits yet), a new git repo is initialized
/// and origin is set to the remote.
pub fn clone_from_remote(remote: &Path, name: &str) -> Repo {
    let dir = tempfile::Builder::new()
        .prefix(&format!("balls-it-{name}-"))
        .tempdir()
        .expect("tempdir");

    // Check if the remote has a main branch
    let has_main = clean_git_command(dir.path())
        .arg("--git-dir")
        .arg(remote)
        .args(["rev-parse", "--verify", "refs/heads/main"])
        .output()
        .is_ok_and(|o| o.status.success());

    if has_main {
        let out = clean_git_command(dir.path())
            .args(["clone", "-q", "--branch", "main"])
            .arg(remote)
            .arg(dir.path())
            .output()
            .expect("git clone");
        assert!(
            out.status.success(),
            "git clone failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    } else {
        // Empty remote: init a fresh repo, add origin, and let the caller
        // push later.
        git(dir.path(), &["init", "-q", "-b", "main"]);
        let remote_str = remote.to_string_lossy().to_string();
        git(dir.path(), &["remote", "add", "origin", &remote_str]);
    }

    git(
        dir.path(),
        &["config", "user.email", &format!("{name}@example.com")],
    );
    git(dir.path(), &["config", "user.name", name]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    // Seed `main` if the remote was empty so tests that push or
    // rev-parse HEAD on the clone do not trip on an unborn branch.
    // XDG bl init no longer writes any commit on `main` (SPEC §14.19),
    // so the empty-remote test path used to inherit `bl init`'s
    // "initial commit" seed and now needs an explicit one.
    if !git_ok(dir.path(), &["rev-parse", "--verify", "--quiet", "refs/heads/main"]) {
        git(dir.path(), &["commit", "--allow-empty", "-qm", "seed", "--no-verify"]);
    }
    Repo { dir, origin_remote: None }
}

/// Read and JSON-parse a task file directly from the store. Layout-
/// aware via [`paths::discover_tasks_dir`].
pub fn read_task_json(repo_root: &Path, id: &str) -> serde_json::Value {
    let path = discover_tasks_dir(repo_root).join(format!("{id}.json"));
    let s = std::fs::read_to_string(&path).expect("read task");
    serde_json::from_str(&s).expect("parse task json")
}

/// Read the sibling notes file for a task as a list of JSON values, one
/// per line. Returns an empty list if the file does not exist.
pub fn read_task_notes(repo_root: &Path, id: &str) -> Vec<serde_json::Value> {
    let path = discover_tasks_dir(repo_root).join(format!("{id}.notes.jsonl"));
    let Ok(s) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse note"))
        .collect()
}

/// Run git against the clone's state checkout, layout-aware. Asserts
/// the command succeeded and the state checkout exists.
pub fn git_state(repo: &Path, args: &[&str]) -> String {
    let sr = discover_state_repo(repo).expect("non-stealth repo has a state checkout");
    git(&sr, args)
}

/// Commit everything pending in the clone's state checkout, layout-
/// aware. No-op on stealth.
pub fn commit_state_repo(repo: &Path, msg: &str) {
    let Some(sr) = discover_state_repo(repo) else {
        return;
    };
    if !sr.join(".git").exists() {
        return;
    }
    git(&sr, &["add", "-A"]);
    if !git_ok(&sr, &["diff", "--cached", "--quiet"]) {
        git(&sr, &["commit", "-m", msg, "--no-verify"]);
    }
}

/// Push current branch (main) to origin, and the `balls/tasks` state
/// branch from the layout-resolved state checkout.
pub fn push(cwd: &Path) {
    git(cwd, &["push", "origin", "main"]);
    let Some(sr) = discover_state_repo(cwd) else {
        return;
    };
    if sr.join(".git").exists()
        && git_ok(&sr, &["rev-parse", "--verify", "--quiet", "refs/heads/balls/tasks"])
    {
        let _ = clean_git_command(&sr)
            .args(["push", "origin", "balls/tasks"])
            .output();
    }
}

/// Pull from origin (fetch + merge).
pub fn pull(cwd: &Path) {
    git(cwd, &["pull", "--no-edit", "origin", "main"]);
}

/// Flip the repo's `core.bare` flag on directly, mimicking a
/// bare-converted clone without going through `bl`.
pub fn set_core_bare(repo_root: &Path) {
    git(repo_root, &["config", "core.bare", "true"]);
}

// XdgRepo + new_xdg_repo() live in `tests/common/xdg_repo.rs` to keep
// this file under the 300-line cap; re-exported above.
