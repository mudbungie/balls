//! Helpers for `Store::init`: the bare-clone bootstrap, the
//! `.gitignore` wiring, and the `.balls/tasks` convenience symlink.
//! Extracted from `store.rs` to keep that file focused on the Store
//! API. The state checkout itself is materialized by `state_repo`.

use crate::config::Config;
use crate::error::{BallError, Result};
use crate::store::Store;
use crate::store_legacy::legacy_with;
use crate::store_paths::init_stealth_tasks;
use crate::tracker_address;
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

impl Store {
    /// Initialize a balls store at `from`. Phase 1A keeps the legacy
    /// in-repo layout — `.balls/` at the clone root, a runtime state
    /// checkout under `.balls/state-repo/`, and the `balls: initialize`
    /// commit on main. Phase 1B (bl-e802) flips this to the XDG layout
    /// (SPEC §5, §14.1) and stops writing on main.
    pub fn init(from: &Path, stealth: bool, tasks_dir: Option<String>) -> Result<Self> {
        if let Some(ref td) = tasks_dir {
            if !Path::new(td).is_absolute() {
                return Err(BallError::Other(format!(
                    "--tasks-dir must be an absolute path, got: {td}"
                )));
            }
        }
        let (repo_root, no_git) = match git::git_root(from) {
            Ok(r) => (r, false),
            Err(BallError::NotARepo) if tasks_dir.is_some() => (
                fs::canonicalize(from).unwrap_or_else(|_| from.to_path_buf()),
                true,
            ),
            Err(e) => return Err(e),
        };
        if !no_git {
            git::git_ensure_user(&repo_root)?;
            git::git_init_commit(&repo_root)?;
        }

        let balls_dir = repo_root.join(".balls");
        let local_dir = balls_dir.join("local");
        let already = balls_dir.join("config.json").exists();
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
        let (tasks_dir_path, state_repo_path, state_branch_name) = if use_stealth {
            let td = init_stealth_tasks(&repo_root, &local_dir, tasks_dir)?;
            (
                td,
                repo_root.join(crate::state_repo::STATE_REPO_REL),
                tracker_address::DEFAULT_BRANCH.to_string(),
            )
        } else {
            let cfg = Config::load(&config_path)?;
            let addr = tracker_address::resolve(&repo_root, &cfg);
            let sr = crate::state_repo::ensure(&repo_root, &addr)?;
            let branch = git::git_current_branch(&sr)
                .unwrap_or_else(|_| tracker_address::DEFAULT_BRANCH.to_string());
            (sr.join(".balls/tasks"), sr, branch)
        };

        if !no_git {
            commit_init(&repo_root, use_stealth, already)?;
        }
        let mut store =
            legacy_with(repo_root, tasks_dir_path, state_repo_path, state_branch_name, use_stealth);
        store.no_git = no_git;
        Ok(store)
    }

    /// Bootstrap a bare clone at `clone_dir` from `source` and open a
    /// Store rooted there. Heavy lifting is in `bootstrap_bare_clone`.
    pub fn init_bare(source: &str, clone_dir: &Path) -> Result<Self> {
        let root = bootstrap_bare_clone(source, clone_dir)?;
        let state_repo_path = root.join(crate::state_repo::STATE_REPO_REL);
        let tasks_dir_path = state_repo_path.join(".balls/tasks");
        let state_branch_name = git::git_current_branch(&state_repo_path)
            .unwrap_or_else(|_| tracker_address::DEFAULT_BRANCH.to_string());
        Ok(legacy_with(
            root,
            tasks_dir_path,
            state_repo_path,
            state_branch_name,
            false,
        ))
    }
}

/// Bootstrap a bare clone at `clone_dir` from `source` (a git
/// URL or path whose `main` carries a balls-initialized project) per
/// SPEC §3: a bare-cloned gitdir plus the loose `.balls/` store and a
/// materialized `.balls/state-repo`. Returns the resolved clone root.
///
/// Idempotent and self-healing: a present bare gitdir is reused (a
/// non-bare `.git` is refused, never clobbered); scaffolding
/// `create_dir_all`s are no-ops when present; `config.json` is
/// materialized only when missing. The state checkout is built by the
/// shared `state_repo::ensure`, which adopts the clone's bare-cloned
/// `balls/tasks` in place — no working-tree commit, no checkout to
/// write a `balls: initialize` to.
pub(crate) fn bootstrap_bare_clone(source: &str, clone_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(clone_dir)?;
    let clone_dir = fs::canonicalize(clone_dir).unwrap_or_else(|_| clone_dir.to_path_buf());
    let gitdir = clone_dir.join(".git");

    // Bare-clone into <clone>/.git. Reuse an existing bare gitdir;
    // refuse to clobber a non-bare one (non-destructive, like bl init).
    if gitdir.exists() {
        if !crate::bare_squash::is_bare_repo(&clone_dir).unwrap_or(false) {
            return Err(BallError::Other(format!(
                "{} exists and is not a bare repo; refusing to clobber it",
                gitdir.display()
            )));
        }
    } else {
        git::git_clone_bare(source, &gitdir)?;
    }
    // Wire remote-tracking so origin/* refs stay current for bl sync.
    git::git_config_set(
        &clone_dir,
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    )?;
    let _ = git::git_fetch(&clone_dir, "origin");
    git::git_ensure_user(&clone_dir)?;

    // Reconstruct the loose store: scaffold the per-clone dirs and
    // materialize the repo-tracked config.json (no checkout exists to
    // copy it from at a bare root).
    let balls = clone_dir.join(".balls");
    for d in ["plugins", "local/claims", "local/lock", "local/plugins"] {
        fs::create_dir_all(balls.join(d))?;
    }
    let config_path = balls.join("config.json");
    if !config_path.exists() {
        let cfg = git_state::show_file(&clone_dir, "main", ".balls/config.json")?.ok_or_else(
            || {
                BallError::Other(
                    "source's `main` has no .balls/config.json — run `bl init` in a \
                 working clone and push first (README bootstrap step 1)"
                        .into(),
                )
            },
        )?;
        fs::write(&config_path, cfg)?;
    }
    let cfg = Config::load(&config_path)?;
    let addr = tracker_address::resolve(&clone_dir, &cfg);
    crate::state_repo::ensure(&clone_dir, &addr)?;
    Ok(clone_dir)
}

/// Expose the state checkout's task directory to the clone via a
/// stable symlink: `<root>/.balls/tasks -> state-repo/.balls/tasks`.
/// An engineer can `ls .balls/tasks/`, `cat`, and `$EDITOR` files
/// through it without any balls-specific knowledge.
///
/// A pre-existing symlink with a different target is repointed (a repo
/// migrated off the legacy `.balls/worktree` otherwise keeps a symlink
/// to the deleted checkout). Matching targets are no-ops.
pub(crate) fn ensure_tasks_symlink(root: &Path, target: &str) -> Result<()> {
    let link = root.join(".balls/tasks");
    let want = PathBuf::from(target);
    if link.is_symlink() {
        if fs::read_link(&link).ok().as_deref() == Some(want.as_path()) {
            return Ok(());
        }
        fs::remove_file(&link)?;
    } else if link.exists() {
        // Pre-existing directory or file at the symlink path is ambiguous:
        // refuse to overwrite to avoid clobbering uncommitted tasks.
        return Err(BallError::Other(format!(
            "unexpected non-symlink at {}; remove it and re-run `bl init`",
            link.display()
        )));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&want, &link)?;
    }
    #[cfg(not(unix))]
    {
        return Err(BallError::Other(
            "symlink-mode bl init requires a POSIX filesystem; use stealth mode".into(),
        ));
    }
    Ok(())
}

/// Commit the init-time files to the clone's code branch:
/// `.gitignore` and the repo-owned `.balls/config.json`. The state
/// checkout's symlinks (`.balls/tasks`, `.balls/plugins`,
/// `.balls/state-repo`) are gitignored runtime state — never staged.
/// Stealth mode has no state checkout, so its real `.balls/plugins/`
/// is committed with a `.gitkeep`.
pub(crate) fn commit_init(root: &Path, is_stealth: bool, already: bool) -> Result<()> {
    crate::gitignore::ensure_main_gitignore(root, is_stealth)?;
    let mut paths: Vec<&Path> = vec![Path::new(".gitignore"), Path::new(".balls/config.json")];
    let keep_rel = Path::new(".balls/plugins/.gitkeep");
    if is_stealth {
        let abs_keep = root.join(keep_rel);
        if !abs_keep.exists() {
            fs::write(&abs_keep, "")?;
        }
        paths.push(keep_rel);
    }
    git::git_add(root, &paths)?;
    let msg = if already { "balls: reinitialize" } else { "balls: initialize" };
    git::git_commit(root, msg)?;
    Ok(())
}

#[cfg(test)]
#[path = "store_init_tests.rs"]
mod tests;
