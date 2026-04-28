//! Shared helpers for integration tests.
//!
//! Tests spin up real git repositories in temp dirs and run the `bl` binary
//! as a subprocess. A typical multi-dev scenario uses a bare "remote" and
//! two cloned working repos as Dev A and Dev B.

#![allow(dead_code)]

pub mod human_gate;
pub mod plugin;

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
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

/// Environment variables that git sets during hooks. Must be cleared so
/// tests using temp repos are fully isolated from the parent repo.
pub const GIT_ENV_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_WORK_TREE",
    "GIT_PREFIX",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

pub fn git(cwd: &Path, args: &[&str]) -> String {
    let mut cmd = StdCommand::new("git");
    cmd.current_dir(cwd).args(args);
    for var in GIT_ENV_VARS {
        cmd.env_remove(var);
    }
    let out = cmd.output().expect("git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub fn git_ok(cwd: &Path, args: &[&str]) -> bool {
    let mut cmd = StdCommand::new("git");
    cmd.current_dir(cwd).args(args);
    for var in GIT_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd.output().is_ok_and(|o| o.status.success())
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
    let mut check = StdCommand::new("git");
    check
        .arg("--git-dir")
        .arg(remote)
        .args(["rev-parse", "--verify", "refs/heads/main"]);
    for var in GIT_ENV_VARS {
        check.env_remove(var);
    }
    let has_main = check.output().is_ok_and(|o| o.status.success());

    if has_main {
        let mut clone_cmd = StdCommand::new("git");
        clone_cmd
            .args(["clone", "-q", "--branch", "main"])
            .arg(remote)
            .arg(dir.path());
        for var in GIT_ENV_VARS {
            clone_cmd.env_remove(var);
        }
        let out = clone_cmd.output().expect("git clone");
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
    for var in GIT_ENV_VARS {
        c.env_remove(var);
    }
    c
}

pub fn bl_as(cwd: &Path, identity: &str) -> Command {
    let mut c = Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd);
    c.env("BALLS_IDENTITY", identity);
    for var in GIT_ENV_VARS {
        c.env_remove(var);
    }
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

/// Push current branch (main) to origin.
pub fn push(cwd: &Path) {
    git(cwd, &["push", "origin", "main"]);
    // Push the state branch too if it exists — mirrors `bl sync`, so
    // tests that round-trip tasks across clones don't need to call sync.
    if git_ok(
        cwd,
        &["rev-parse", "--verify", "--quiet", "refs/heads/balls/tasks"],
    ) {
        let _ = StdCommand::new("git")
            .current_dir(cwd)
            .args(["push", "origin", "balls/tasks"])
            .env_remove("GIT_DIR")
            .output();
    }
}

/// Pull from origin (fetch + merge).
pub fn pull(cwd: &Path) {
    git(cwd, &["pull", "--no-edit", "origin", "main"]);
}
