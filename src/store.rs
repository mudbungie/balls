use crate::config::Config;
use crate::error::{BallError, Result};
use crate::project_config::ProjectConfig;
use crate::git;
use crate::store_paths::find_main_root;
use crate::task::{self, Task};
use crate::xdg_discover;
use std::path::{Path, PathBuf};

// flock primitives live in `store_lock` to keep this file under the
// 300-line cap. `task_lock`/`LockGuard` are re-exported so the public
// API path `balls::store::{task_lock, LockGuard}` is unchanged.
pub use crate::store_lock::{task_lock, LockGuard};

/// Which on-disk layout the resolved Store is operating against —
/// SPEC-clone-layout's nested XDG layout (the new shape), or the
/// pre-XDG in-repo layout (`.balls/` colocated, `.balls-worktrees/`
/// in tree). Phase 1A reads either; Phase 1B (bl init) writes only
/// `Xdg`. The discriminant is consulted by the few sites that need
/// layout-specific behavior (`bl migrate`, `bl doctor`); the path
/// accessors return the resolved field so most callers do not care.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Nested XDG layout per SPEC-clone-layout §3 — `trackers/`,
    /// `worktrees/`, `claims/`, `locks/`, `plugins-auth/` under
    /// `~/.local/state/balls/`; `clone.json` under `~/.config/balls/`.
    Xdg,
    /// Pre-XDG in-repo layout: `.balls/config.json` committed to main,
    /// `.balls/state-repo/` runtime checkout, `.balls-worktrees/` in
    /// tree. Phase 1 reads this when no XDG state is materialized;
    /// `bl migrate` (Phase 2, bl-717e) relocates it.
    Legacy,
}

pub struct Store {
    pub root: PathBuf,
    pub stealth: bool,
    /// True when no git repository is available. Implies stealth.
    pub no_git: bool,
    /// Which on-disk layout `discover` resolved against (§12 dual-read).
    pub layout: Layout,
    pub(crate) tasks_dir_path: PathBuf,
    /// Where state-branch git ops run — the tracker checkout under XDG
    /// (`~/.local/state/balls/trackers/<enc-origin>/<enc-branch>/`) or
    /// `.balls/state-repo` under legacy. Meaningless in stealth mode.
    pub(crate) state_repo_path: PathBuf,
    /// The tracker's state branch (SPEC-tracker-state §5), cached at
    /// discovery.
    pub(crate) state_branch_name: String,
    /// Layout-aware per-clone path fields. Populated at discovery /
    /// init; accessor methods return the resolved value so callers
    /// stay layout-agnostic. Under legacy these are all under
    /// `<root>/.balls/`; under XDG they live under XDG bases.
    pub(crate) claims_dir_path: PathBuf,
    pub(crate) lock_dir_path: PathBuf,
    pub(crate) local_plugins_dir_path: PathBuf,
    pub(crate) worktrees_root_path: PathBuf,
    pub(crate) local_dir_path: PathBuf,
    pub(crate) config_file_path: PathBuf,
    pub(crate) project_config_file_path: PathBuf,
}

impl Store {
    /// Discover the project root from a starting directory.
    /// In a worktree, returns the main repo root so that all writes go there.
    pub fn discover(from: &Path) -> Result<Self> {
        let store = match Self::discover_git(from) {
            Err(BallError::NotARepo) => crate::store_legacy::discover_no_git(from)?,
            other => other?,
        };
        crate::pending_sync_legacy::warn_if_present(&store);
        Ok(store)
    }

    fn discover_git(from: &Path) -> Result<Self> {
        crate::store_paths::require_git_repo(from)?;
        let common_dir = git::git_common_dir(from)?;
        let main_root = find_main_root(&common_dir)?;
        git::git_ensure_user(&main_root)?;
        // SPEC §12 row 2: prefer the new layout when present, fall
        // back to legacy with a one-line nudge to migrate.
        if let Some(store) = xdg_discover::try_open(&main_root)? {
            return Ok(store);
        }
        if !main_root.join(".balls").exists() {
            return Err(BallError::git_repo_no_balls(&main_root));
        }
        crate::store_legacy::discover(&main_root)
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.tasks_dir_path.clone()
    }

    pub fn local_dir(&self) -> PathBuf {
        self.local_dir_path.clone()
    }

    pub fn claims_dir(&self) -> PathBuf {
        self.claims_dir_path.clone()
    }

    pub fn lock_dir(&self) -> PathBuf {
        self.lock_dir_path.clone()
    }

    pub fn local_plugins_dir(&self) -> PathBuf {
        self.local_plugins_dir_path.clone()
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_file_path.clone()
    }

    /// The clone's repo config. Legacy: a real `.balls/config.json`.
    /// XDG: a `repo.json` on the tracker branch — read through the
    /// same `Config` schema so call sites stay unchanged.
    pub fn load_config(&self) -> Result<Config> {
        Config::load(&self.config_path())
    }

    pub fn project_config_path(&self) -> PathBuf {
        self.project_config_file_path.clone()
    }

    /// The project's config (SPEC §7). Legacy: read through the
    /// `.balls/project.json` symlink. XDG: read from the tracker
    /// checkout's `.balls/project.json` directly.
    pub fn load_project_config(&self) -> Result<ProjectConfig> {
        ProjectConfig::resolve(&self.project_config_path(), &self.config_path())
    }

    /// Root that a plugin's `config_file` path is joined against. The
    /// `.balls/plugins` symlink redirects into the state checkout, so
    /// the clone root always works.
    pub fn plugin_config_root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn worktrees_root(&self) -> Result<PathBuf> {
        // XDG: the per-clone worktrees path is resolved at discover.
        // Legacy: honor the configured `worktree_dir` override
        // (`.balls/config.json` field). New XDG clones cannot override
        // the worktrees path — the layout is the layout (SPEC §13).
        match self.layout {
            Layout::Xdg => Ok(self.worktrees_root_path.clone()),
            Layout::Legacy => {
                let cfg = self.load_config()?;
                Ok(self.root.join(cfg.worktree_dir))
            }
        }
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
// `all_tasks` — are a second `impl Store` block in `store_archive.rs`.
// `init`/`init_bare` live in `store_init.rs`. Legacy in-repo discovery
// (the SPEC §12 dual-read fallback) lives in `store_legacy.rs`. All
// extracted to keep this file under the 300-line cap.
