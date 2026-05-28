//! The unified state checkout — `.balls/state-repo/` (SPEC-tracker-state §6).
//!
//! Every clone, standalone or federated, resolves ONE checkout:
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
use crate::project_config::ProjectConfig;
use crate::tracker_address::Address;
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

// The `.balls/plugins` and `.balls/project.json` symlink materializers
// live in `state_repo_symlinks`; re-exported so callers are unchanged.
pub(crate) use crate::state_repo_symlinks::{ensure_plugins_symlink, ensure_project_json_symlink};
use crate::state_repo_migrate::{guard_legacy_worktree, retire_legacy_worktree};

/// Relative path (from the repo root) of the balls-owned state clone.
pub(crate) const STATE_REPO_REL: &str = ".balls/state-repo";

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
    seed(root, &dir)?;
    crate::store_init::ensure_tasks_symlink(root, "state-repo/.balls/tasks")?;
    ensure_plugins_symlink(root, "state-repo/.balls/plugins")?;
    ensure_project_json_symlink(root, "state-repo/.balls/project.json")?;
    // bl-73bb: commit after the symlink steps so a legacy
    // `.balls/plugins/*` absorbed by `ensure_plugins_symlink` lands on
    // the tracker branch alongside the seed scaffolding instead of
    // dangling untracked.
    if git::has_uncommitted_changes(&dir)? {
        git::git_add_all(&dir)?;
        git::git_commit(&dir, "balls: seed state branch")?;
    }
    // bl-de57 (code-branch companion to bl-73bb): after the absorb,
    // drop the now-orphan `.balls/plugins/*` index entries from the
    // clone's code branch and refresh `.gitignore` for the unified
    // runtime paths. A no-op on a clone that never carried the
    // legacy layout.
    crate::legacy_plugin_migrate::run(root)?;
    Ok(dir)
}

/// Warm path: the checkout already exists. Keep `origin` and the
/// checked-out branch aligned with the address (a hand-edited
/// `state_url`/`state_branch`, or a `bl remaster --branch B`) but
/// never fetch — discover stays offline-fast; `bl sync`/`bl prime` do
/// the network round-trip.
fn warm(dir: &Path, addr: &Address) -> Result<()> {
    git::git_ensure_user(dir)?;
    if let Some(url) = &addr.url {
        // `set_remote` is the add-or-replace primitive — collapses
        // legacy bl-remaster's "URL rotated" path and the gained-an-
        // origin path into one covered line.
        let _ = git_state::set_remote(dir, "origin", url);
    }
    align_branch(dir, &addr.branch)?;
    Ok(())
}

/// Align the state checkout's HEAD with `branch` — the configured
/// `state_branch` (SPEC §5). A no-op when HEAD already matches; a
/// `git checkout -B branch` when not, which both points the local
/// branch at the current commit (preserving local-only tasks for the
/// seed case) and switches HEAD to it. Lets a `bl remaster --branch
/// B` re-target without nuking the checkout. `pub` so `Store`'s warm
/// fast-path can call it without going through the full `ensure`.
pub fn align_warm_branch(dir: &Path, branch: &str) -> Result<()> {
    align_branch(dir, branch)
}

fn align_branch(dir: &Path, branch: &str) -> Result<()> {
    if git::git_current_branch(dir).is_ok_and(|h| h == branch) {
        return Ok(());
    }
    run_at(dir, &["checkout", "-q", "-B", branch])
}

/// First contact: no `.balls/state-repo` yet. Adopt a legacy worktree
/// in place, clone the tracker, or `git init` a local orphan.
fn first_contact(root: &Path, dir: &Path, addr: &Address) -> Result<()> {
    // Migration: a legacy standalone repo carries `balls/tasks` in its
    // own git. Adopt it locally — offline, no clone — onto the freshly
    // `init`-ed (unborn) state branch, and retire `.balls/worktree`.
    if git_state::branch_exists(root, &addr.branch) {
        guard_legacy_worktree(root, &addr.branch)?;
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

/// Seed `.balls/tasks/` scaffolding, the `.balls/plugins/` dir, and the
/// `.balls/project.json` project config on the state branch. The commit
/// that gives a fresh branch its HEAD is made by `ensure` after the
/// symlink steps, so an absorbed legacy `.balls/plugins/*` is captured
/// in the same commit instead of landing untracked.
fn seed(root: &Path, state_repo: &Path) -> Result<()> {
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
    seed_project_config(root, state_repo)?;
    Ok(())
}

/// Materialize `.balls/project.json` on the tracker branch once (SPEC
/// §6.3 / §7). For a repo predating the config split this is the
/// migration: the project-owned fields a `config.json` still carries —
/// the `plugins` map above all — are copied into `project.json` so
/// they survive the move off the code branch.
fn seed_project_config(root: &Path, state_repo: &Path) -> Result<()> {
    let project_json = state_repo.join(".balls/project.json");
    if project_json.exists() {
        return Ok(());
    }
    ProjectConfig::from_config_file(&root.join(".balls/config.json")).save(&project_json)
}

/// Bring a *warm* state checkout up to the SPEC §7 config split. A
/// checkout materialized before `project.json` existed (a repo updated
/// past bl-8a9a but not yet bl-e609) is otherwise never re-`seed`ed —
/// `Store::discover` skips `ensure` once `.balls/state-repo` is warm.
/// This runs on the warm path instead: it materializes `project.json`
/// on the tracker branch (migrating the pre-split `config.json`) and
/// its clone symlink. A no-op — two `stat`s — once both exist.
pub fn ensure_project_config(root: &Path, state_repo: &Path) -> Result<()> {
    let link = root.join(".balls/project.json");
    if link.is_symlink() && link.exists() {
        return Ok(());
    }
    seed_project_config(root, state_repo)?;
    if git::has_uncommitted_changes(state_repo)? {
        git::git_add_all(state_repo)?;
        git::git_commit(state_repo, "balls: migrate project config")?;
    }
    ensure_project_json_symlink(root, "state-repo/.balls/project.json")
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
pub(crate) mod test_support;
#[cfg(test)]
#[path = "state_repo_tests.rs"]
mod tests;
