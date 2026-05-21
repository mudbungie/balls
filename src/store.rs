use crate::config::Config;
use crate::error::{BallError, Result};
use crate::git;
use crate::store_init::{bootstrap_bare_hub, commit_init, setup_state_branch};
use crate::store_paths::{
    auto_provision_master, find_balls_root, find_main_root, init_stealth_tasks, resolve_layout,
};
use crate::task::{self, Task};
use crate::task_io;
use std::fs;
use std::path::{Path, PathBuf};

// flock primitives live in `store_lock` to keep this file under the
// 300-line cap. `task_lock`/`LockGuard` are re-exported so the public
// API path `balls::store::{task_lock, LockGuard}` is unchanged.
pub use crate::store_lock::{task_lock, LockGuard};
use crate::store_lock::state_worktree_flock;

pub struct Store {
    pub root: PathBuf,
    pub stealth: bool,
    /// True when no git repository is available. Implies stealth.
    pub no_git: bool,
    tasks_dir_path: PathBuf,
    /// Where state-branch git ops run. `.balls/state-repo` under
    /// `master_url`, `.balls/worktree` otherwise (bl-ffb4 seam).
    state_worktree_path: PathBuf,
}

impl Store {
    /// Discover the project root from a starting directory.
    /// In a worktree, returns the main repo root so that all writes go there.
    pub fn discover(from: &Path) -> Result<Self> {
        match Self::discover_git(from) {
            Err(BallError::NotARepo) => Self::discover_no_git(from),
            other => other,
        }
    }

    fn discover_git(from: &Path) -> Result<Self> {
        crate::store_paths::require_git_repo(from)?;
        let common_dir = git::git_common_dir(from)?;
        let main_root = find_main_root(&common_dir)?;
        if !main_root.join(".balls").exists() {
            return Err(BallError::git_repo_no_balls(&main_root));
        }
        // Auto-provision the balls-owned state-repo on first discover
        // after a fresh `git clone` of a master_url-configured project.
        // Idempotent; offline-safe (failure leaves discover to surface
        // its own diagnostic).
        auto_provision_master(&main_root);
        let (tasks_dir_path, stealth, state_worktree_path) = resolve_layout(&main_root);
        if !stealth && !tasks_dir_path.exists() {
            return Err(BallError::state_worktree_missing(&main_root, &tasks_dir_path));
        }
        git::git_ensure_user(&main_root)?;
        Ok(Store { root: main_root, stealth, no_git: false, tasks_dir_path, state_worktree_path })
    }

    fn discover_no_git(from: &Path) -> Result<Self> {
        let root = find_balls_root(from)?;
        let (tasks_dir_path, stealth, state_worktree_path) = resolve_layout(&root);
        if !stealth || !tasks_dir_path.exists() {
            return Err(BallError::no_git_store_unusable(&root, &tasks_dir_path, stealth));
        }
        Ok(Store { root, stealth, no_git: true, tasks_dir_path, state_worktree_path })
    }

    pub fn init(from: &Path, stealth: bool, tasks_dir: Option<String>) -> Result<Self> {
        if let Some(ref td) = tasks_dir {
            if !Path::new(td).is_absolute() {
                return Err(BallError::Other(format!("--tasks-dir must be an absolute path, got: {td}")));
            }
        }
        let (repo_root, no_git) = match git::git_root(from) {
            Ok(r) => (r, false),
            Err(BallError::NotARepo) if tasks_dir.is_some() => {
                (fs::canonicalize(from).unwrap_or_else(|_| from.to_path_buf()), true)
            }
            Err(e) => return Err(e),
        };
        if !no_git {
            git::git_ensure_user(&repo_root)?;
            git::git_init_commit(&repo_root)?;
        }

        let balls_dir = repo_root.join(".balls");
        let local_dir = balls_dir.join("local");
        let already = balls_dir.join("config.json").exists();
        fs::create_dir_all(balls_dir.join("plugins"))?;
        fs::create_dir_all(local_dir.join("claims"))?;
        fs::create_dir_all(local_dir.join("lock"))?;
        fs::create_dir_all(local_dir.join("plugins"))?;
        let config_path = balls_dir.join("config.json");
        if !config_path.exists() {
            Config::default().save(&config_path)?;
        }

        let use_stealth = stealth || tasks_dir.is_some();
        let (tasks_dir_path, is_stealth, state_worktree_path) = if use_stealth {
            init_stealth_tasks(&repo_root, &local_dir, tasks_dir)?
        } else {
            // master_url overrides the project-worktree leg entirely:
            // balls owns its own clone, project's `.git/config` stays
            // untouched (bl-ffb4).
            let cfg = Config::load(&config_path)?;
            let wt = if let Some(url) = cfg.master_url() {
                crate::state_repo::ensure(&repo_root, url)?
            } else {
                setup_state_branch(&repo_root, cfg.state_remote(), cfg.state_remote.is_some())?;
                repo_root.join(".balls/worktree")
            };
            (wt.join(".balls/tasks"), false, wt)
        };

        if !no_git {
            commit_init(&repo_root, is_stealth, already)?;
        }
        Ok(Store { root: repo_root, stealth: is_stealth, no_git, tasks_dir_path, state_worktree_path })
    }

    /// Bootstrap a bare central hub at `hubdir` from `source` and open
    /// a Store rooted there. Heavy lifting is in `bootstrap_bare_hub`.
    pub fn init_bare(source: &str, hubdir: &Path) -> Result<Self> {
        let root = bootstrap_bare_hub(source, hubdir)?;
        let (tasks_dir_path, _, state_worktree_path) = resolve_layout(&root);
        Ok(Store { root, stealth: false, no_git: false, tasks_dir_path, state_worktree_path })
    }

    pub fn balls_dir(&self) -> PathBuf {
        self.root.join(".balls")
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.tasks_dir_path.clone()
    }

    /// Directory where git operations against task state should run.
    /// In non-stealth mode this is the resolved state checkout —
    /// `.balls/state-repo/` when `master_url` is set (balls-owned clone,
    /// bl-ffb4) or `.balls/worktree/` otherwise (legacy worktree of
    /// project repo). In stealth mode the concept is meaningless —
    /// callers branch on `stealth` first.
    pub fn state_worktree_dir(&self) -> PathBuf {
        self.state_worktree_path.clone()
    }

    pub fn local_dir(&self) -> PathBuf {
        self.balls_dir().join("local")
    }

    pub fn claims_dir(&self) -> PathBuf {
        self.local_dir().join("claims")
    }

    pub fn lock_dir(&self) -> PathBuf {
        self.local_dir().join("lock")
    }

    pub fn local_plugins_dir(&self) -> PathBuf {
        self.local_dir().join("plugins")
    }

    pub fn config_path(&self) -> PathBuf {
        self.balls_dir().join("config.json")
    }

    pub fn load_config(&self) -> Result<Config> {
        Config::load(&self.config_path())
    }

    pub fn worktrees_root(&self) -> Result<PathBuf> {
        let cfg = self.load_config()?;
        Ok(self.root.join(cfg.worktree_dir))
    }

    pub fn task_path(&self, id: &str) -> Result<PathBuf> {
        task::validate_id(id)?;
        Ok(self.tasks_dir().join(format!("{id}.json")))
    }

    pub fn task_exists(&self, id: &str) -> bool {
        self.task_path(id).is_ok_and(|p| p.exists())
    }

    pub fn load_task(&self, id: &str) -> Result<Task> {
        let p = self.task_path(id)?;
        if !p.exists() {
            return Err(BallError::TaskNotFound(id.to_string()));
        }
        Task::load(&p)
    }

    /// Persist a task. Callers must ensure serialization (typically via the
    /// per-task lock helper in `worktree.rs`); this path relies on atomic
    /// tmp+rename for filesystem integrity.
    pub fn save_task(&self, task: &Task) -> Result<()> {
        task.save(&self.task_path(&task.id)?)
    }

    /// Remove a task's files from disk and (non-stealth) from the
    /// state branch's index. Unified replacement for the previous
    /// `delete_task_file` + `rm_task_git` pair.
    /// Stage and commit a task file change on the state branch. No-op
    /// in stealth mode. Stages the sibling notes file too (always
    /// present after a `Task::save`). Holds the store-wide
    /// state-worktree lock for the duration of the git ops.
    pub fn commit_task(&self, id: &str, message: &str) -> Result<()> {
        if self.stealth {
            return Ok(());
        }
        let _g = state_worktree_flock(self)?;
        let dir = self.state_worktree_dir();
        let json = PathBuf::from(format!(".balls/tasks/{id}.json"));
        let notes = PathBuf::from(format!(".balls/tasks/{id}.notes.jsonl"));
        git::git_add(&dir, &[json.as_path(), notes.as_path()])?;
        git::git_commit(&dir, message)?;
        Ok(())
    }

    /// Archive a task and commit the archive on the state branch in a
    /// single locked sequence. Replaces the old `archive_task` +
    /// `commit_staged` pair, which could interleave with another
    /// worker's writes between the `git rm` and the `git commit`.
    ///
    /// The caller has already mutated `task` (status=Closed,
    /// closed_at set, etc.) but must NOT have called `save_task` —
    /// this method handles parent-side bookkeeping and file removal
    /// atomically under the state-worktree lock.
    pub fn close_and_archive(&self, task: &Task, commit_msg: &str) -> Result<()> {
        let mut parent_resaved = false;
        if let Some(pid) = &task.parent {
            if let Ok(mut parent) = self.load_task(pid) {
                parent.closed_children.push(task::ArchivedChild {
                    id: task.id.clone(),
                    title: task.title.clone(),
                    closed_at: task.closed_at.unwrap_or_else(chrono::Utc::now),
                    extra: std::collections::BTreeMap::new(),
                });
                parent.touch();
                self.save_task(&parent)?;
                parent_resaved = true;
            }
        }
        if self.stealth {
            let p = self.task_path(&task.id)?;
            if p.exists() { fs::remove_file(&p)?; }
            task_io::delete_notes_file(&p)?;
            return Ok(());
        }
        let _g = state_worktree_flock(self)?;
        let dir = self.state_worktree_dir();
        if let Some(pid) = task.parent.as_ref().filter(|_| parent_resaved) {
            // Stage the parent's notes sidecar alongside its json: it
            // exists post-`save_task` (mirrors `commit_task`), is a
            // no-op stage when unchanged, and carries the reject note
            // into this same commit on the deferred-reject path. We
            // gate on `parent_resaved` because an already-archived
            // parent leaves no file in the state worktree — staging
            // it would abort close on a missing pathspec.
            let pj = PathBuf::from(format!(".balls/tasks/{pid}.json"));
            let pn = PathBuf::from(format!(".balls/tasks/{pid}.notes.jsonl"));
            git::git_add(&dir, &[pj.as_path(), pn.as_path()])?;
        }
        let json = PathBuf::from(format!(".balls/tasks/{}.json", task.id));
        let notes = PathBuf::from(format!(".balls/tasks/{}.notes.jsonl", task.id));
        git::git_rm_force(&dir, &[json.as_path(), notes.as_path()])?;
        git::git_commit(&dir, commit_msg)?;
        Ok(())
    }

    pub fn all_tasks(&self) -> Result<Vec<Task>> {
        let dir = self.tasks_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            match Task::load(&path) {
                Ok(t) => out.push(t),
                Err(e) => {
                    // Surface malformed but don't abort on one bad file
                    eprintln!("warning: malformed task {}: {}", path.display(), e);
                }
            }
        }
        Ok(out)
    }
}
