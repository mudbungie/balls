//! The unified state checkout — `.balls/state-repo/` (SPEC-tracker-state §6).
//!
//! Every workspace, standalone or federated, resolves ONE checkout:
//! a balls-owned git clone of the tracker address at
//! `.balls/state-repo/`. `Store` materializes it from the resolved
//! `Address` (`tracker_address::resolve`); there is no mode flag and
//! no second layout. "Standalone" is just the case where the address
//! is the code repo's own `origin`.
//!
//! Reachability (§9): first contact with an unreachable *explicit*
//! tracker hard-fails — silently dropping to a local orphan would let
//! task changes drift from the project. The implicit default (no
//! `state_url`) falls back to a local `git init`; a warm checkout
//! soft-fails offline.
//!
//! Migration: a legacy standalone repo whose `balls/tasks` lives in
//! its own git (with a `.balls/worktree` checkout) is migrated in
//! place — the branch is fetched into the new `.balls/state-repo` and
//! the legacy worktree retired.

use crate::error::{BallError, Result};
use crate::tracker_address::Address;
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

// The `.balls/plugins` symlink materializer lives in
// `state_repo_symlinks`; re-exported so callers are unchanged.
pub(crate) use crate::state_repo_symlinks::ensure_plugins_symlink;

/// Relative path (from the repo root) of the balls-owned state clone.
pub(crate) const STATE_REPO_REL: &str = ".balls/state-repo";

/// The legacy project-worktree checkout retired by the unified model.
const LEGACY_WORKTREE_REL: &str = ".balls/worktree";

/// Materialize `.balls/state-repo/` for `addr`. Idempotent: a warm
/// checkout is reused with no network round-trip; a missing one is
/// built (clone, local-fallback, or in-place legacy migration).
pub fn ensure(root: &Path, addr: &Address) -> Result<PathBuf> {
    let dir = root.join(STATE_REPO_REL);
    if dir.join(".git").exists() {
        warm(&dir, addr)?;
    } else {
        first_contact(root, &dir, addr)?;
    }
    seed(&dir)?;
    crate::store_init::ensure_tasks_symlink(root, "state-repo/.balls/tasks")?;
    ensure_plugins_symlink(root, "state-repo/.balls/plugins")?;
    Ok(dir)
}

/// Warm path: the checkout already exists. Keep `origin` aligned with
/// the address (a hand-edited `state_url`) but never fetch — discover
/// stays offline-fast; `bl sync`/`bl prime` do the network round-trip.
fn warm(dir: &Path, addr: &Address) -> Result<()> {
    git::git_ensure_user(dir)?;
    if let Some(url) = &addr.url {
        if git::git_has_remote(dir, "origin") {
            let _ = git::git_config_set(dir, "remote.origin.url", url);
        } else {
            let _ = run_at(dir, &["remote", "add", "origin", url]);
        }
    }
    Ok(())
}

/// First contact: no `.balls/state-repo` yet. Adopt a legacy worktree
/// in place, clone the tracker, or `git init` a local orphan.
fn first_contact(root: &Path, dir: &Path, addr: &Address) -> Result<()> {
    // Migration: a legacy standalone repo carries `balls/tasks` in its
    // own git. Adopt it locally — offline, no clone — onto the freshly
    // `init`-ed (unborn) state branch, and retire `.balls/worktree`.
    if git_state::branch_exists(root, &addr.branch) {
        init_repo(dir, &addr.branch, addr.url.as_deref())?;
        run_at(dir, &["fetch", &root.to_string_lossy(), &addr.branch])?;
        git::git_reset_hard(dir, "FETCH_HEAD")?;
        retire_legacy_worktree(root, &addr.branch);
        return Ok(());
    }
    if let Some(url) = &addr.url {
        return clone_from_url(dir, addr, url);
    }
    // Implicit default with no `origin`: a solo offline repo.
    init_repo(dir, &addr.branch, None)?;
    git_state::create_orphan_branch(dir, &addr.branch, "balls state")?;
    checkout(dir, &addr.branch)
}

/// Materialize from a tracker URL. Online: track the remote branch, or
/// create+publish an orphan if the tracker has none. Offline: hard-fail
/// an explicit address, local-fallback an implicit one (§9).
fn clone_from_url(dir: &Path, addr: &Address, url: &str) -> Result<()> {
    init_repo(dir, &addr.branch, Some(url))?;
    let (online, fetch_err) = fetch_origin(dir)?;
    if !online {
        if addr.explicit {
            // Roll the scaffold back so a retry is a clean first contact.
            let _ = fs::remove_dir_all(dir);
            return Err(unreachable_tracker_err(url, &fetch_err));
        }
        git_state::create_orphan_branch(dir, &addr.branch, "balls state")?;
        return checkout(dir, &addr.branch);
    }
    if git_state::has_remote_branch(dir, "origin", &addr.branch) {
        git_state::create_tracking_branch(dir, &addr.branch, "origin")?;
    } else {
        git_state::create_orphan_branch(dir, &addr.branch, "balls state")?;
        // Best-effort first publish; a divergent tracker rejects the
        // non-force push and we stay safe-but-unlinked.
        let _ = git::git_push(dir, "origin", &addr.branch);
    }
    checkout(dir, &addr.branch)
}

/// `git init` the state clone, with `origin` wired to `url` if given.
fn init_repo(dir: &Path, branch: &str, url: Option<&str>) -> Result<()> {
    fs::create_dir_all(dir)?;
    run_at(
        dir.parent().unwrap_or(dir),
        &["init", "-q", "--initial-branch", branch, &dir.to_string_lossy()],
    )?;
    if let Some(u) = url {
        run_at(dir, &["remote", "add", "origin", u])?;
    }
    git::git_ensure_user(dir)
}

/// Retire the legacy `.balls/worktree`: drop the linked worktree, its
/// registry entry, and the project git's now-adopted `balls/tasks`
/// branch. Entirely best-effort — migration succeeds on the clone.
fn retire_legacy_worktree(root: &Path, branch: &str) {
    let wt = root.join(LEGACY_WORKTREE_REL);
    let _ = git::git_worktree_remove(root, &wt, true);
    let _ = git_state::worktree_prune(root);
    if wt.exists() {
        let _ = fs::remove_dir_all(&wt);
    }
    let _ = git::git_branch_delete(root, branch, true);
}

/// Capture success and stderr from `git fetch origin`. The stderr is
/// folded into the hard-fail diagnostic so the user can tell "host
/// unreachable" from "permission denied" from "ref not found".
fn fetch_origin(dir: &Path) -> Result<(bool, String)> {
    let out = git::run_git_in(dir, &["fetch", "origin"])?;
    Ok(if out.status.success() {
        (true, String::new())
    } else {
        (false, String::from_utf8_lossy(&out.stderr).trim().to_string())
    })
}

fn unreachable_tracker_err(url: &str, fetch_err: &str) -> BallError {
    BallError::Other(format!(
        "could not reach state tracker `{url}`\n  underlying error: {fetch_err}\n  \
         A configured state_url must be reachable for first-time setup — \
         silently dropping to a local orphan would let your task changes \
         drift away from the rest of the project. Options:\n  \
         - Fix access (credentials, VPN, URL typo) and re-run.\n  \
         - Edit state_url in .balls/config.json to point at a tracker you can reach.\n  \
         - Run `bl remaster --detach` to drop the tracker link and work standalone."
    ))
}

fn checkout(dir: &Path, branch: &str) -> Result<()> {
    run_at(dir, &["checkout", "-q", branch])
}

fn run_at(dir: &Path, args: &[&str]) -> Result<()> {
    let status = git::clean_git_command(dir)
        .args(args)
        .status()
        .map_err(|e| BallError::Git(format!("git {}: {e}", args.join(" "))))?;
    if !status.success() {
        return Err(BallError::Git(format!("git {} exited with {status}", args.join(" "))));
    }
    Ok(())
}

/// Seed `.balls/tasks/` scaffolding and the `.balls/plugins/` dir on
/// the state branch, committing anything new so the branch has a HEAD.
fn seed(state_repo: &Path) -> Result<()> {
    let tasks = state_repo.join(".balls/tasks");
    fs::create_dir_all(&tasks)?;
    let attrs = tasks.join(".gitattributes");
    let need_attrs = match fs::read_to_string(&attrs) {
        Ok(s) => !s.contains("*.notes.jsonl merge=union"),
        Err(_) => true,
    };
    if need_attrs {
        fs::write(&attrs, "*.notes.jsonl merge=union\n")?;
    }
    for keep in [tasks.join(".gitkeep"), state_repo.join(".balls/plugins/.gitkeep")] {
        if let Some(parent) = keep.parent() {
            fs::create_dir_all(parent)?;
        }
        if !keep.exists() {
            fs::write(&keep, "")?;
        }
    }
    if git::has_uncommitted_changes(state_repo)? {
        git::git_add_all(state_repo)?;
        git::git_commit(state_repo, "balls: seed state branch")?;
    }
    Ok(())
}

/// Detect URL-shaped `bl remaster` targets so a bare git-remote name
/// can be resolved against the project's `.git/config` instead.
pub fn looks_like_url(s: &str) -> bool {
    s.contains("://")
        || s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("../")
        || ssh_shorthand(s)
}

fn ssh_shorthand(s: &str) -> bool {
    let Some((host, path)) = s.split_once(':') else {
        return false;
    };
    !host.is_empty()
        && !path.is_empty()
        && host.contains('@')
        && !path.contains(' ')
        && !s.contains("://")
}

#[cfg(test)]
#[path = "state_repo_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "state_repo_tests.rs"]
mod tests;
