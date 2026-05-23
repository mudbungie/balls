//! Helpers for `Store::init`: the bare-workspace bootstrap, the
//! `.gitignore` wiring, and the `.balls/tasks` convenience symlink.
//! Extracted from `store.rs` to keep that file focused on the Store
//! API. The state checkout itself is materialized by `state_repo`.

use crate::config::Config;
use crate::error::{BallError, Result};
use crate::tracker_address;
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

/// Bootstrap a bare workspace at `workspace_dir` from `source` (a git
/// URL or path whose `main` carries a balls-initialized project) per
/// SPEC §3: a bare-cloned gitdir plus the loose `.balls/` store and a
/// materialized `.balls/state-repo`. Returns the resolved workspace
/// root.
///
/// Idempotent and self-healing: a present bare gitdir is reused (a
/// non-bare `.git` is refused, never clobbered); scaffolding
/// `create_dir_all`s are no-ops when present; `config.json` is
/// materialized only when missing. The state checkout is built by the
/// shared `state_repo::ensure`, which adopts the workspace's
/// bare-cloned `balls/tasks` in place — no working-tree commit, no
/// checkout to write a `balls: initialize` to.
pub(crate) fn bootstrap_bare_workspace(source: &str, workspace_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(workspace_dir)?;
    let workspace_dir =
        fs::canonicalize(workspace_dir).unwrap_or_else(|_| workspace_dir.to_path_buf());
    let gitdir = workspace_dir.join(".git");

    // Bare-clone into <workspace>/.git. Reuse an existing bare gitdir;
    // refuse to clobber a non-bare one (non-destructive, like bl init).
    if gitdir.exists() {
        if !crate::bare_squash::is_bare_repo(&workspace_dir).unwrap_or(false) {
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
        &workspace_dir,
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    )?;
    let _ = git::git_fetch(&workspace_dir, "origin");
    git::git_ensure_user(&workspace_dir)?;

    // Reconstruct the loose store: scaffold the per-workspace dirs and
    // materialize the workspace-tracked config.json (no checkout
    // exists to copy it from at a bare root).
    let balls = workspace_dir.join(".balls");
    for d in ["plugins", "local/claims", "local/lock", "local/plugins"] {
        fs::create_dir_all(balls.join(d))?;
    }
    let config_path = balls.join("config.json");
    if !config_path.exists() {
        let cfg =
            git_state::show_file(&workspace_dir, "main", ".balls/config.json")?.ok_or_else(
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
    let addr = tracker_address::resolve(&workspace_dir, &cfg);
    crate::state_repo::ensure(&workspace_dir, &addr)?;
    Ok(workspace_dir)
}

/// Expose the state checkout's task directory to the workspace via a
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

/// Commit the init-time files to the workspace's code branch:
/// `.gitignore` and the workspace-owned `.balls/config.json`. The
/// state checkout's symlinks (`.balls/tasks`, `.balls/plugins`,
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
