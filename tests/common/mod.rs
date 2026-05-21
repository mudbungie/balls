//! Shared helpers for integration tests.
//!
//! Tests spin up real git repositories in temp dirs and run the `bl` binary
//! as a subprocess. A typical multi-dev scenario uses a bare "remote" and
//! two cloned working repos as Dev A and Dev B.

#![allow(dead_code, unused_imports)]

mod config_seed;
pub mod forge;
pub mod human_gate;
pub mod multidev;
pub mod native_plugin;
pub mod plugin;
pub mod tracker;

pub use config_seed::{seed_config, set_project_plugins};

use assert_cmd::Command;
use balls::git::clean_git_command;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A pair of (temp dir, repo root path). The TempDir must be kept alive to
/// prevent cleanup.
pub struct Repo {
    pub dir: TempDir,
}

impl Repo {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
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
/// initial branch "main".
pub fn new_repo() -> Repo {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "dev@example.com"]);
    git(dir.path(), &["config", "user.name", "Test Dev"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    Repo { dir }
}

/// Create a bare repository at a temp path to act as a remote.
pub fn new_bare_remote() -> Repo {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "--bare", "-b", "main"]);
    Repo { dir }
}

/// Clone a bare remote into a fresh temp dir as a developer workspace.
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
    Repo { dir }
}

/// Return the path to the compiled `bl` binary.
pub fn bl_bin() -> PathBuf {
    // assert_cmd handles this: Command::cargo_bin("bl")
    PathBuf::from(env!("CARGO_BIN_EXE_bl"))
}

pub fn bl(cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd);
    c.env("BALLS_IDENTITY", "test-user");
    c
}

pub fn bl_as(cwd: &Path, identity: &str) -> Command {
    let mut c = Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd);
    c.env("BALLS_IDENTITY", identity);
    c
}

/// Run `bl create TITLE` and return the newly created ID (parsed from stdout).
pub fn create_task(cwd: &Path, title: &str) -> String {
    let out = bl(cwd)
        .args(["create", title])
        .output()
        .expect("bl create");
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

pub fn init_in(cwd: &Path) {
    bl(cwd).arg("init").assert().success();
}

/// Read and JSON-parse a task file directly from the store.
pub fn read_task_json(repo_root: &Path, id: &str) -> serde_json::Value {
    let path = repo_root.join(".balls/tasks").join(format!("{id}.json"));
    let s = std::fs::read_to_string(&path).expect("read task");
    serde_json::from_str(&s).expect("parse task json")
}

/// Read the sibling notes file for a task as a list of JSON values, one
/// per line. Returns an empty list if the file does not exist.
pub fn read_task_notes(repo_root: &Path, id: &str) -> Vec<serde_json::Value> {
    let path = repo_root
        .join(".balls/tasks")
        .join(format!("{id}.notes.jsonl"));
    let Ok(s) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse note"))
        .collect()
}

/// Run git against the workspace's state checkout (`.balls/state-repo`),
/// where the `balls/tasks` branch and its history live under the
/// unified model. The asserting sibling of `git`.
pub fn git_state(repo: &Path, args: &[&str]) -> String {
    git(&repo.join(".balls/state-repo"), args)
}

/// Commit everything pending in the workspace's state checkout
/// (`.balls/state-repo`). Under the unified model `.balls/plugins`
/// resolves into that checkout, so plugin config files written by the
/// test helpers are committed here, not on the code branch.
pub fn commit_state_repo(repo: &Path, msg: &str) {
    let sr = repo.join(".balls/state-repo");
    if !sr.join(".git").exists() {
        return; // a stealth repo has no state checkout
    }
    git(&sr, &["add", "-A"]);
    if !git_ok(&sr, &["diff", "--cached", "--quiet"]) {
        git(&sr, &["commit", "-m", msg, "--no-verify"]);
    }
}

/// Push current branch (main) to origin.
pub fn push(cwd: &Path) {
    git(cwd, &["push", "origin", "main"]);
    // Push the state branch too if it exists — mirrors `bl sync`, so
    // tests that round-trip tasks across clones don't need to call
    // sync. Under the unified model `balls/tasks` lives in the
    // `.balls/state-repo` checkout, whose own `origin` is the tracker.
    let sr = cwd.join(".balls/state-repo");
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

/// Run `bl doctor` and return stdout. Asserts exit 0 — doctor is
/// read-only and never fails the process; the verdict is in the text.
pub fn doctor(cwd: &Path) -> String {
    let out = bl(cwd).arg("doctor").output().expect("bl doctor");
    assert!(out.status.success(), "doctor must exit 0 (read-only)");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Flip the repo's `core.bare` flag on directly, mimicking a
/// bare-converted hub without going through `bl`.
pub fn set_core_bare(repo_root: &Path) {
    git(repo_root, &["config", "core.bare", "true"]);
}

/// Run `bl show --json` for a (possibly archived) task and return the
/// parsed value. Asserts the command succeeded.
pub fn show_json(repo: &Path, id: &str) -> serde_json::Value {
    let out = bl(repo).args(["show", id, "--json"]).output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

// `seed_config` / `set_project_plugins` live in `config_seed` (line-cap
// split) and are re-exported above.
