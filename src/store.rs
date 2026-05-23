use crate::config::Config;
use crate::error::{BallError, Result};
use crate::project_config::ProjectConfig;
use crate::git;
use crate::store_init::{bootstrap_bare_workspace, commit_init};
use crate::store_paths::{find_balls_root, find_main_root, init_stealth_tasks, stealth_tasks_override};
use crate::task::{self, Task};
use crate::tracker_address;
use std::fs;
use std::path::{Path, PathBuf};

// flock primitives live in `store_lock` to keep this file under the
// 300-line cap. `task_lock`/`LockGuard` are re-exported so the public
// API path `balls::store::{task_lock, LockGuard}` is unchanged.
pub use crate::store_lock::{task_lock, LockGuard};

pub struct Store {
    pub root: PathBuf,
    pub stealth: bool,
    /// True when no git repository is available. Implies stealth.
    pub no_git: bool,
    tasks_dir_path: PathBuf,
    /// Where state-branch git ops run — the unified `.balls/state-repo`
    /// checkout for every non-stealth repo. A meaningless sentinel in
    /// stealth mode; callers branch on `stealth` first.
    pub(crate) state_repo_path: PathBuf,
    /// The tracker's state branch (SPEC-tracker-state §5), cached at
    /// discovery from `.balls/state-repo`'s HEAD. Default in stealth.
    pub(crate) state_branch_name: String,
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
        git::git_ensure_user(&main_root)?;
        if let Some(tasks_dir_path) = stealth_tasks_override(&main_root) {
            let state_repo_path = main_root.join(crate::state_repo::STATE_REPO_REL);
            return Ok(Store {
                root: main_root,
                stealth: true,
                no_git: false,
                tasks_dir_path,
                state_repo_path,
                state_branch_name: tracker_address::DEFAULT_BRANCH.to_string(),
            });
        }
        // The unified model resolves ONE checkout, `.balls/state-repo`,
        // materialized from the tracker address (SPEC §6). An
        // unreachable explicit tracker hard-fails here (§9).
        let state_repo_path = Self::ensure_state_repo(&main_root)?;
        let tasks_dir_path = state_repo_path.join(".balls/tasks");
        if !tasks_dir_path.exists() {
            return Err(BallError::state_worktree_missing(&main_root, &tasks_dir_path));
        }
        let state_branch_name = resolve_state_branch(&state_repo_path);
        Ok(Store {
            root: main_root,
            stealth: false,
            no_git: false,
            tasks_dir_path,
            state_repo_path,
            state_branch_name,
        })
    }

    /// Materialize `.balls/state-repo` if absent, returning its path.
    /// A warm checkout is returned with best-effort project-config
    /// migration and branch alignment — no network round-trip.
    /// Failures in the warm fast path fall through silently so a
    /// broken state-repo or corrupt config stays discoverable for
    /// `bl doctor` to surface the real problem.
    fn ensure_state_repo(root: &Path) -> Result<PathBuf> {
        let dir = root.join(crate::state_repo::STATE_REPO_REL);
        if dir.join(".git").exists() {
            crate::state_repo::ensure_project_config(root, &dir)?;
            if let Ok(cfg) = Config::load(&root.join(".balls/config.json")) {
                let addr = tracker_address::resolve(root, &cfg);
                let _ = crate::state_repo::align_warm_branch(&dir, &addr.branch);
            }
            return Ok(dir);
        }
        let cfg = Config::load(&root.join(".balls/config.json"))?;
        let addr = tracker_address::resolve(root, &cfg);
        crate::state_repo::ensure(root, &addr)
    }

    fn discover_no_git(from: &Path) -> Result<Self> {
        let root = find_balls_root(from)?;
        let tasks_dir = stealth_tasks_override(&root);
        if let Some(p) = tasks_dir.as_ref().filter(|p| p.exists()) {
            return Ok(Store {
                state_repo_path: root.join(crate::state_repo::STATE_REPO_REL),
                tasks_dir_path: p.clone(),
                root,
                stealth: true,
                no_git: true,
                state_branch_name: tracker_address::DEFAULT_BRANCH.to_string(),
            });
        }
        let had = tasks_dir.is_some();
        let shown = tasks_dir.unwrap_or_else(|| root.join(".balls/tasks"));
        Err(BallError::no_git_store_unusable(&root, &shown, had))
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
        // On a non-stealth repo `.balls/plugins` becomes a symlink into
        // the state checkout; a re-init must not `create_dir_all`
        // through that symlink (it dangles until `ensure` rebuilds).
        let plugins = balls_dir.join("plugins");
        if !plugins.is_symlink() {
            fs::create_dir_all(&plugins)?;
        }
        fs::create_dir_all(local_dir.join("claims"))?;
        fs::create_dir_all(local_dir.join("lock"))?;
        fs::create_dir_all(local_dir.join("plugins"))?;
        let config_path = balls_dir.join("config.json");
        if !config_path.exists() {
            Config::default().save(&config_path)?;
        }

        let use_stealth = stealth || tasks_dir.is_some();
        let default_branch = || tracker_address::DEFAULT_BRANCH.to_string();
        let (tasks_dir_path, state_repo_path, state_branch_name) = if use_stealth {
            let td = init_stealth_tasks(&repo_root, &local_dir, tasks_dir)?;
            (td, repo_root.join(crate::state_repo::STATE_REPO_REL), default_branch())
        } else {
            let cfg = Config::load(&config_path)?;
            let addr = tracker_address::resolve(&repo_root, &cfg);
            let sr = crate::state_repo::ensure(&repo_root, &addr)?;
            let branch = resolve_state_branch(&sr);
            (sr.join(".balls/tasks"), sr, branch)
        };

        if !no_git {
            commit_init(&repo_root, use_stealth, already)?;
        }
        Ok(Store {
            root: repo_root,
            stealth: use_stealth,
            no_git,
            tasks_dir_path,
            state_repo_path,
            state_branch_name,
        })
    }

    /// Bootstrap a bare workspace at `workspace_dir` from `source` and
    /// open a Store rooted there. Heavy lifting is in
    /// `bootstrap_bare_workspace`.
    pub fn init_bare(source: &str, workspace_dir: &Path) -> Result<Self> {
        let root = bootstrap_bare_workspace(source, workspace_dir)?;
        let state_repo_path = root.join(crate::state_repo::STATE_REPO_REL);
        let tasks_dir_path = state_repo_path.join(".balls/tasks");
        let state_branch_name = resolve_state_branch(&state_repo_path);
        Ok(Store {
            root,
            stealth: false,
            no_git: false,
            tasks_dir_path,
            state_repo_path,
            state_branch_name,
        })
    }

    pub fn balls_dir(&self) -> PathBuf {
        self.root.join(".balls")
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.tasks_dir_path.clone()
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

    /// The workspace's config. `config.json` is a real, never-symlinked
    /// workspace file under the unified model (SPEC §7), so the load
    /// is a plain read — no symlink-into-the-tracker indirection, no
    /// per-owner merge.
    pub fn load_config(&self) -> Result<Config> {
        Config::load(&self.config_path())
    }

    /// Path of the project config — `.balls/project.json`, a symlink
    /// into the state checkout for every non-stealth repo (SPEC §7).
    pub fn project_config_path(&self) -> PathBuf {
        self.balls_dir().join("project.json")
    }

    /// The project's config (SPEC §7): the schema version, id width,
    /// `min_bl_version` floor, and plugin map shared by every workspace
    /// on the tracker. `.balls/project.json` resolves through a symlink
    /// into the state checkout; a repo without one — stealth, or a
    /// checkout predating the config split — falls its project-owned
    /// fields back to `config.json`.
    pub fn load_project_config(&self) -> Result<ProjectConfig> {
        ProjectConfig::resolve(&self.project_config_path(), &self.config_path())
    }

    /// Root that a plugin's `config_file` path is joined against. The
    /// `.balls/plugins` symlink redirects into the state checkout, so
    /// the workspace root always works.
    pub fn plugin_config_root(&self) -> PathBuf {
        self.root.clone()
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
}

// The state-branch write methods — `commit_task`, `close_and_archive`,
// `all_tasks` — are a second `impl Store` block in `store_archive.rs`,
// split out to keep this file under the 300-line cap.

/// Resolve the state branch from `.balls/state-repo`'s HEAD. Falls
/// back to the SPEC default for stealth/no-git (no checkout exists).
fn resolve_state_branch(state_repo: &Path) -> String {
    git::git_current_branch(state_repo)
        .unwrap_or_else(|_| tracker_address::DEFAULT_BRANCH.to_string())
}
