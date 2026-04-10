use crate::config::Config;
use crate::error::{BallError, Result};
use crate::git;
use crate::task::Task;
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
    fs::create_dir_all(store.lock_dir())?;
    let lock_path = store.lock_dir().join(format!("{}.lock", id));
    let f = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    f.lock_exclusive()?;
    Ok(LockGuard(f))
}

pub struct Store {
    /// The main repo root (git-common-dir's parent, effectively the primary checkout)
    pub root: PathBuf,
}

impl Store {
    /// Discover the ball root from a starting directory.
    /// In a worktree, returns the main repo root so that all writes go there.
    pub fn discover(from: &Path) -> Result<Self> {
        // Find git_root (may be worktree) then resolve to main repo via git-common-dir
        let _worktree_root = git::git_root(from)?;
        let common_dir = git::git_common_dir(from)?;
        // common_dir typically ends in .git (main repo) or worktrees/<name> (worktree)
        // The main worktree's path = parent of .git
        let main_root = find_main_root(&common_dir)?;
        let ball_dir = main_root.join(".ball");
        if !ball_dir.exists() {
            return Err(BallError::NotInitialized);
        }
        Ok(Store { root: main_root })
    }

    pub fn init(from: &Path) -> Result<Self> {
        let repo_root = git::git_root(from)?;
        git::git_ensure_user(&repo_root)?;

        // Ensure we have at least one commit for worktree operations later
        git::git_init_commit(&repo_root)?;

        let ball_dir = repo_root.join(".ball");
        let tasks_dir = ball_dir.join("tasks");
        let plugins_dir = ball_dir.join("plugins");
        let local_dir = ball_dir.join("local");
        let local_claims = local_dir.join("claims");
        let local_lock = local_dir.join("lock");
        let local_plugins = local_dir.join("plugins");

        let already = ball_dir.join("config.json").exists();

        fs::create_dir_all(&tasks_dir)?;
        fs::create_dir_all(&plugins_dir)?;
        fs::create_dir_all(&local_claims)?;
        fs::create_dir_all(&local_lock)?;
        fs::create_dir_all(&local_plugins)?;

        let config_path = ball_dir.join("config.json");
        if !config_path.exists() {
            Config::default().save(&config_path)?;
        }

        // Ensure .gitignore has the entries
        let gitignore_path = repo_root.join(".gitignore");
        let mut gitignore = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path)?
        } else {
            String::new()
        };
        // Use patterns without trailing slash so they also cover symlinks —
        // worktrees symlink .ball/local, and git's directory-only match would
        // otherwise treat that symlink as untracked.
        let need_local = !gitignore.lines().any(|l| l.trim() == ".ball/local");
        let need_wt = !gitignore.lines().any(|l| l.trim() == ".ball-worktrees");
        if need_local || need_wt {
            if !gitignore.is_empty() && !gitignore.ends_with('\n') {
                gitignore.push('\n');
            }
            if need_local {
                gitignore.push_str(".ball/local\n");
            }
            if need_wt {
                gitignore.push_str(".ball-worktrees\n");
            }
            fs::write(&gitignore_path, gitignore)?;
        }

        // Tasks dir placeholder so it's always present after clone
        let keep = tasks_dir.join(".gitkeep");
        if !keep.exists() {
            fs::write(&keep, "")?;
        }
        let plugins_keep = plugins_dir.join(".gitkeep");
        if !plugins_keep.exists() {
            fs::write(&plugins_keep, "")?;
        }

        git::git_add(
            &repo_root,
            &[
                Path::new(".ball/config.json"),
                Path::new(".ball/tasks/.gitkeep"),
                Path::new(".ball/plugins/.gitkeep"),
                Path::new(".gitignore"),
            ],
        )?;

        if already {
            // Was already initialized; commit only if something is actually staged
            git::git_commit(&repo_root, "ball: reinitialize")?;
        } else {
            git::git_commit(&repo_root, "ball: initialize")?;
        }

        Ok(Store { root: repo_root })
    }

    pub fn ball_dir(&self) -> PathBuf {
        self.root.join(".ball")
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.ball_dir().join("tasks")
    }

    pub fn local_dir(&self) -> PathBuf {
        self.ball_dir().join("local")
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
        self.ball_dir().join("config.json")
    }

    pub fn load_config(&self) -> Result<Config> {
        Config::load(&self.config_path())
    }

    pub fn worktrees_root(&self) -> Result<PathBuf> {
        let cfg = self.load_config()?;
        Ok(self.root.join(cfg.worktree_dir))
    }

    pub fn task_path(&self, id: &str) -> PathBuf {
        self.tasks_dir().join(format!("{}.json", id))
    }

    pub fn task_exists(&self, id: &str) -> bool {
        self.task_path(id).exists()
    }

    pub fn load_task(&self, id: &str) -> Result<Task> {
        let p = self.task_path(id);
        if !p.exists() {
            return Err(BallError::TaskNotFound(id.to_string()));
        }
        Task::load(&p)
    }

    /// Persist a task. Callers must ensure serialization (typically via the
    /// per-task lock helper in `worktree.rs`); this path relies on atomic
    /// tmp+rename for filesystem integrity.
    pub fn save_task(&self, task: &Task) -> Result<()> {
        task.save(&self.task_path(&task.id))
    }

    pub fn delete_task_file(&self, id: &str) -> Result<()> {
        let p = self.task_path(id);
        if p.exists() {
            std::fs::remove_file(&p)?;
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

fn find_main_root(common_dir: &Path) -> Result<PathBuf> {
    // common_dir is either:
    //   .../repo/.git  (main)
    //   .../repo/.git/worktrees/<name>  (worktree's own git dir, but common-dir resolves to main's .git)
    // git-common-dir resolves to main's .git, so parent is always the main root.
    let canon = fs::canonicalize(common_dir).unwrap_or_else(|_| common_dir.to_path_buf());
    canon
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| BallError::Other("could not find main repo root".to_string()))
}
