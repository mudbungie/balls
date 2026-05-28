use crate::clone_json::CloneJson;
use crate::config::Config;
use crate::effective_config::EffectiveConfig;
use crate::encoding::nested_clone_path;
use crate::error::{BallError, Result};
use crate::project_config::ProjectConfig;
use crate::git;
use crate::store_effective;
use crate::store_paths::find_main_root;
use crate::task::{self, Task};
use crate::xdg_discover;
use crate::xdg_paths::XdgBases;
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
    pub(crate) config_file_path: PathBuf,
    pub(crate) project_config_file_path: PathBuf,
    /// Parsed `clone.json` for this on-disk checkout (SPEC §6.4).
    /// Populated under XDG when the file exists; `None` under legacy
    /// (clone.json is XDG-only — legacy clones used `.balls/local/
    /// config.json` which bl-5a03 retired) and under XDG when no
    /// per-clone overrides have been set.
    pub(crate) clone_json: Option<CloneJson>,
}

impl Store {
    /// Discover the project root from a starting directory.
    /// In a worktree, returns the main repo root so that all writes go there.
    pub fn discover(from: &Path) -> Result<Self> {
        let store = match Self::discover_git(from) {
            Err(BallError::NotARepo) => {
                // SPEC §4.1: a stealth XDG clone has no git and resolves
                // via `clone.json` keyed by the cwd (or --tasks-dir).
                if let Some(store) = xdg_discover::try_open(from)? {
                    store
                } else {
                    crate::store_legacy::discover_no_git(from)?
                }
            }
            other => other?,
        };
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

    /// This on-disk checkout's `clone.json` overrides (SPEC §6.4).
    /// `None` ⇒ no per-clone override is in effect (either no file or
    /// the clone is on the legacy layout, where clone.json has no
    /// place to live).
    pub fn clone_json(&self) -> Option<&CloneJson> {
        self.clone_json.as_ref()
    }

    /// SPEC §3 cache root: `~/.cache/balls/<nested-clone-path>/`,
    /// honoring `XDG_CACHE_HOME`. Used for regenerable per-clone
    /// markers (`last_fetch`, bl-5814). When `HOME`/`XDG_CACHE_HOME`
    /// are both unset, roots at the clone; the marker still works.
    /// Callers create the directory lazily on first write.
    pub fn cache_dir(&self) -> PathBuf {
        let bases = XdgBases::from_env()
            .unwrap_or_else(|| XdgBases::with(&self.root, None, None, None));
        bases.cache_root().join(nested_clone_path(&self.root))
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

    /// Single seam for layered-field reads (SPEC §6.5). Replaces
    /// the pre-Phase-1B-6 `load_config` + XDG `Config`-shaped
    /// synthesizer pair. Under XDG, resolved from `repo.json` +
    /// `project.json` + `clone.json` per the §6.5 merger. Under
    /// Legacy, the on-disk `.balls/config.json` is adapted into the
    /// post-XDG shape so callers stay layout-agnostic.
    pub fn load_effective_config(&self) -> Result<EffectiveConfig> {
        store_effective::load(self)
    }

    /// Resolved integration branch for a task with the given optional
    /// per-task `target_branch`. Routes the full SPEC §6.7 chain
    /// (`task.target_branch ?? legacy-repo.target_branch ?? HEAD@root`)
    /// — the middle layer applies under Legacy only.
    pub fn integration_branch_for(&self, task_target: Option<&str>) -> Result<String> {
        store_effective::integration_branch_for(self, task_target)
    }

    /// Repo-level integration branch with no per-task override.
    /// Shorthand for `integration_branch_for(None)`.
    pub fn integration_branch(&self) -> Result<String> {
        self.integration_branch_for(None)
    }

    /// Explicit repo-level `target_branch` override. Always `None`
    /// under XDG (SPEC §6.7 removed the field); under Legacy returns
    /// the parsed `Config::target_branch` when set. `bl review`
    /// deferred mode consults this to validate that the PR base is
    /// unambiguous before pushing the work branch.
    pub fn explicit_repo_target_branch(&self) -> Result<Option<String>> {
        store_effective::explicit_repo_target_branch(self)
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

    /// Root that a plugin's `config_file` path is joined against.
    /// Legacy: the clone root (`.balls/plugins` symlink → state
    /// checkout). XDG: the tracker checkout (no `.balls/` lives at
    /// the clone root, SPEC §14.1).
    pub fn plugin_config_root(&self) -> PathBuf {
        match self.layout {
            Layout::Legacy => self.root.clone(),
            Layout::Xdg => self.state_repo_path.clone(),
        }
    }

    pub fn worktrees_root(&self) -> Result<PathBuf> {
        // XDG: the per-clone worktrees path is resolved at discover.
        // Legacy: honor the configured `worktree_dir` override
        // (`.balls/config.json` field). New XDG clones cannot override
        // the worktrees path — the layout is the layout (SPEC §13).
        match self.layout {
            Layout::Xdg => Ok(self.worktrees_root_path.clone()),
            Layout::Legacy => Ok(self.root.join(Config::load(&self.config_path())?.worktree_dir)),
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

impl Store {
    /// XDG `bl init` per SPEC-clone-layout §3, §5, §6 (Phase 1B).
    /// Body in [`crate::store_init_xdg`] to keep this file under the
    /// 300-line cap. Production entrypoint (`cmd_init`) routes here;
    /// in-source tests keep using the legacy `Store::init` until
    /// their HOME-isolation seam lands.
    pub fn init_xdg(from: &Path, stealth: bool, tasks_dir: Option<String>) -> Result<Self> {
        crate::store_init_xdg::init(from, stealth, tasks_dir)
    }
}

// The state-branch write methods — `commit_task`, `close_and_archive`,
// `all_tasks` — are a second `impl Store` block in `store_archive.rs`.
// `init`/`init_bare` live in `store_init.rs`. Legacy in-repo discovery
// (the SPEC §12 dual-read fallback) lives in `store_legacy.rs`. All
// extracted to keep this file under the 300-line cap.

