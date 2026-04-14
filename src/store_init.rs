//! Helpers for `Store::init`: state branch creation, state worktree
//! setup, and main-checkout gitignore wiring. Extracted from `store.rs`
//! to keep that file focused on the Store API.

use crate::error::Result;
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const STATE_BRANCH: &str = "balls/tasks";

/// Relative path (from the repo root) of the state worktree's checkout.
pub(crate) const STATE_WORKTREE_REL: &str = ".balls/worktree";

/// Ensure the orphan state branch exists, is checked out at
/// `.balls/worktree/`, has its schema scaffolding (`.gitattributes`,
/// `.gitkeep`) seeded, and is exposed to the main checkout via a stable
/// `.balls/tasks` symlink.
pub(crate) fn setup_state_branch(root: &Path) -> Result<()> {
    let has_origin = git::git_has_remote(root, "origin");
    if !git_state::branch_exists(root, STATE_BRANCH) {
        // Prefer tracking the remote copy: two `bl init`s in separate
        // clones must not each create a fresh orphan, or their histories
        // will be unrelated and sync will refuse to merge them.
        if has_origin {
            let _ = git::git_fetch(root, "origin");
        }
        if has_origin && git_state::has_remote_branch(root, "origin", STATE_BRANCH) {
            git_state::create_tracking_branch(root, STATE_BRANCH, "origin")?;
        } else {
            git_state::create_orphan_branch(root, STATE_BRANCH, "balls state")?;
            // Best-effort publish so a second clone's init discovers the
            // branch on the remote and tracks it instead of creating its
            // own unrelated orphan. Offline? Fine — first sync will push.
            if has_origin {
                let _ = git::git_push(root, "origin", STATE_BRANCH);
            }
        }
    }

    let state_wt = root.join(STATE_WORKTREE_REL);
    if !state_wt.join(".git").exists() {
        if let Some(parent) = state_wt.parent() {
            fs::create_dir_all(parent)?;
        }
        git_state::worktree_add_existing(root, &state_wt, STATE_BRANCH)?;
    }

    seed_state_worktree(&state_wt)?;
    ensure_tasks_symlink(root)?;

    // Publish the seed commit if we made one above and the branch is
    // freshly-local. Best-effort.
    if has_origin {
        let _ = git::git_push(root, "origin", STATE_BRANCH);
    }
    Ok(())
}

/// Seed the state worktree's task directory on first setup: create the
/// `.balls/tasks/` directory, drop in the `.gitattributes` rule that
/// activates git's built-in union merge driver for notes sidecars, and
/// commit anything new so the state branch has a valid HEAD.
fn seed_state_worktree(state_wt: &Path) -> Result<()> {
    let tasks = state_wt.join(".balls/tasks");
    fs::create_dir_all(&tasks)?;

    let attrs = tasks.join(".gitattributes");
    let attrs_line = "*.notes.jsonl merge=union\n";
    let need_attrs = match fs::read_to_string(&attrs) {
        Ok(s) => !s.contains("*.notes.jsonl merge=union"),
        Err(_) => true,
    };
    if need_attrs {
        fs::write(&attrs, attrs_line)?;
    }

    let keep = tasks.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, "")?;
    }

    if git::has_uncommitted_changes(state_wt)? {
        git::git_add_all(state_wt)?;
        git::git_commit(state_wt, "balls: seed state branch")?;
    }
    Ok(())
}

/// Expose the state worktree's task directory to the main checkout via a
/// stable symlink: `<root>/.balls/tasks -> worktree/.balls/tasks`. An
/// engineer on main can `ls .balls/tasks/`, `cat`, and `$EDITOR` files
/// through this symlink without any balls-specific knowledge.
fn ensure_tasks_symlink(root: &Path) -> Result<()> {
    let link = root.join(".balls/tasks");
    if link.is_symlink() {
        return Ok(());
    }
    if link.exists() {
        // Pre-existing directory or file at the symlink path is ambiguous:
        // refuse to overwrite to avoid clobbering uncommitted tasks.
        return Err(crate::error::BallError::Other(format!(
            "unexpected non-symlink at {}; remove it and re-run `bl init`",
            link.display()
        )));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(PathBuf::from("worktree/.balls/tasks"), &link)?;
    }
    #[cfg(not(unix))]
    {
        return Err(crate::error::BallError::Other(
            "symlink-mode bl init requires a POSIX filesystem; use stealth mode".into(),
        ));
    }
    Ok(())
}

/// Add `.balls/local`, `.balls-worktrees`, and (non-stealth only)
/// `.balls/tasks` + `.balls/worktree` to the main checkout's gitignore.
pub(crate) fn ensure_main_gitignore(root: &Path, is_stealth: bool) -> Result<()> {
    let path = root.join(".gitignore");
    let mut content = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let mut wanted: Vec<&str> = vec![".balls/local", ".balls-worktrees"];
    if !is_stealth {
        wanted.push(".balls/tasks");
        wanted.push(".balls/worktree");
    }
    let mut dirty = false;
    for entry in wanted {
        if !content.lines().any(|l| l.trim() == entry) {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(entry);
            content.push('\n');
            dirty = true;
        }
    }
    if dirty {
        fs::write(&path, content)?;
    }
    Ok(())
}
