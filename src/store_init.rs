//! Helpers for `Store::init`: the bare-clone bootstrap, the
//! `.gitignore` wiring, and the `.balls/tasks` convenience symlink.
//! Extracted from `store.rs` to keep that file focused on the Store
//! API. The state checkout itself is materialized by `state_repo`.

use crate::config::Config;
use crate::error::{BallError, Result};
use crate::tracker_address;
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

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
