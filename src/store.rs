use crate::config::Config;
use crate::error::{BallError, Result};
use crate::git;
use crate::store_init::{ensure_main_gitignore, setup_state_branch, STATE_WORKTREE_REL};
use crate::store_paths::{find_main_root, resolve_tasks_dir, stealth_tasks_dir};
use crate::task::{self, Task};
use crate::task_io;
use fs2::FileExt;
use std::fs;
use std::path::{Path, PathBuf};

/// Acquire an exclusive flock on the given path. The lock is released when
/// the returned guard is dropped.
pub struct LockGuard(fs::File);
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub fn task_lock(store: &Store, id: &str) -> Result<LockGuard> {
    task::validate_id(id)?;
    fs::create_dir_all(store.lock_dir())?;
    let lock_path = store.lock_dir().join(format!("{}.lock", id));
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    f.lock_exclusive()?;
    Ok(LockGuard(f))
}

pub struct Store {
    /// The main repo root (git-common-dir's parent, effectively the primary checkout)
    pub root: PathBuf,
    /// True when tasks live outside the repo (stealth mode).
    pub stealth: bool,
    /// Resolved tasks directory (may be external in stealth mode).
    tasks_dir_path: PathBuf,
}

impl Store {
    /// Discover the project root from a starting directory.
    /// In a worktree, returns the main repo root so that all writes go there.
    pub fn discover(from: &Path) -> Result<Self> {
        let _worktree_root = git::git_root(from)?;
        let common_dir = git::git_common_dir(from)?;
        let main_root = find_main_root(&common_dir)?;
        let balls_dir = main_root.join(".balls");
        if !balls_dir.exists() {
            return Err(BallError::NotInitialized);
        }
        let (tasks_dir_path, stealth) = resolve_tasks_dir(&main_root);
        // Non-stealth mode requires the state worktree to exist. If it's
        // missing, the user cloned without running `bl init` — fail fast
        // with a clear message instead of letting reads/writes land in
        // limbo.
        if !stealth && !tasks_dir_path.exists() {
            return Err(BallError::NotInitialized);
        }
        Ok(Store { root: main_root, stealth, tasks_dir_path })
    }

    pub fn init(from: &Path, stealth: bool) -> Result<Self> {
        let repo_root = git::git_root(from)?;
        git::git_ensure_user(&repo_root)?;
        git::git_init_commit(&repo_root)?;

        let balls_dir = repo_root.join(".balls");
        let plugins_dir = balls_dir.join("plugins");
        let local_dir = balls_dir.join("local");
        let already = balls_dir.join("config.json").exists();

        fs::create_dir_all(&plugins_dir)?;
        fs::create_dir_all(local_dir.join("claims"))?;
        fs::create_dir_all(local_dir.join("lock"))?;
        fs::create_dir_all(local_dir.join("plugins"))?;

        let config_path = balls_dir.join("config.json");
        if !config_path.exists() {
            Config::default().save(&config_path)?;
        }

        let (tasks_dir_path, is_stealth) = if stealth {
            let ext = stealth_tasks_dir(&repo_root);
            fs::create_dir_all(&ext)?;
            fs::write(local_dir.join("tasks_dir"), ext.to_string_lossy().as_bytes())?;
            (ext, true)
        } else {
            setup_state_branch(&repo_root)?;
            (repo_root.join(".balls/worktree/.balls/tasks"), false)
        };

        ensure_main_gitignore(&repo_root, is_stealth)?;
        let plugins_keep = plugins_dir.join(".gitkeep");
        if !plugins_keep.exists() {
            fs::write(&plugins_keep, "")?;
        }

        let paths: Vec<&Path> = vec![
            Path::new(".balls/config.json"),
            Path::new(".balls/plugins/.gitkeep"),
            Path::new(".gitignore"),
        ];
        git::git_add(&repo_root, &paths)?;
        let msg = if already { "balls: reinitialize" } else { "balls: initialize" };
        git::git_commit(&repo_root, msg)?;

        Ok(Store { root: repo_root, stealth: is_stealth, tasks_dir_path })
    }

    pub fn balls_dir(&self) -> PathBuf {
        self.root.join(".balls")
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.tasks_dir_path.clone()
    }

    /// Directory where git operations against task state should run.
    /// In non-stealth mode this is the state worktree (commits land on
    /// the `balls/tasks` orphan branch, never on main). In stealth mode
    /// the concept is meaningless — callers should branch on `stealth`
    /// before using this.
    pub fn state_worktree_dir(&self) -> PathBuf {
        self.root.join(STATE_WORKTREE_REL)
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
        Ok(self.tasks_dir().join(format!("{}.json", id)))
    }

    pub fn task_exists(&self, id: &str) -> bool {
        self.task_path(id).map(|p| p.exists()).unwrap_or(false)
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

    pub fn delete_task_file(&self, id: &str) -> Result<()> {
        let p = self.task_path(id)?;
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
        task_io::delete_notes_file(&p)?;
        Ok(())
    }

    /// Stage and commit a task file change on the state branch. No-op
    /// in stealth mode. Also stages the sibling notes file if it exists.
    pub fn commit_task(&self, id: &str, message: &str) -> Result<()> {
        if self.stealth {
            return Ok(());
        }
        let dir = self.state_worktree_dir();
        let rel_json = PathBuf::from(".balls/tasks").join(format!("{}.json", id));
        git::git_add(&dir, &[rel_json.as_path()])?;
        let notes_abs = task_io::notes_path_for(&self.task_path(id)?);
        if notes_abs.exists() {
            let rel_notes =
                PathBuf::from(".balls/tasks").join(format!("{}.notes.jsonl", id));
            git::git_add(&dir, &[rel_notes.as_path()])?;
        }
        git::git_commit(&dir, message)?;
        Ok(())
    }

    /// Commit whatever is already staged on the state branch. No-op in
    /// stealth mode.
    pub fn commit_staged(&self, message: &str) -> Result<()> {
        if self.stealth {
            return Ok(());
        }
        git::git_commit(&self.state_worktree_dir(), message)?;
        Ok(())
    }

    /// Git-rm a task file on the state branch. No-op in stealth mode.
    /// Also removes the sibling notes file from the index if tracked.
    pub fn rm_task_git(&self, id: &str) -> Result<()> {
        if self.stealth {
            return Ok(());
        }
        let dir = self.state_worktree_dir();
        let rel_json = PathBuf::from(format!(".balls/tasks/{}.json", id));
        git::git_rm(&dir, &[rel_json.as_path()])?;
        let rel_notes = PathBuf::from(format!(".balls/tasks/{}.notes.jsonl", id));
        if dir.join(&rel_notes).exists() {
            let _ = git::git_rm(&dir, &[rel_notes.as_path()]);
        }
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
