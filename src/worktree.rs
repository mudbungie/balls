//! Worktree scaffolding: claim, drop, and orphan cleanup. The submit
//! side (review, close, archive) lives in `review.rs` to keep both
//! files under the 300-line cap.

use crate::error::{BallError, Result};
use crate::policy::ClaimPolicy;
use crate::store::{task_lock, Store};
use crate::task::{self, Status};
use crate::{claim_sync, git};
use std::{fs, path::PathBuf};

pub(crate) fn with_task_lock<T>(
    store: &Store,
    id: &str,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let _guard = task_lock(store, id)?;
    f()
}

pub(crate) fn claim_file_path(store: &Store, id: &str) -> PathBuf {
    store.claims_dir().join(id)
}

fn write_claim_file(store: &Store, id: &str, worker: &str) -> Result<()> {
    fs::create_dir_all(store.claims_dir())?;
    let content = format!(
        "worker={}\npid={}\nclaimed_at={}\n",
        worker,
        std::process::id(),
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    fs::write(claim_file_path(store, id), content)?;
    Ok(())
}

pub(crate) fn worktree_path(store: &Store, id: &str) -> Result<PathBuf> {
    task::validate_id(id)?;
    Ok(store.worktrees_root()?.join(id))
}

pub fn create_worktree(
    store: &Store,
    id: &str,
    identity: &str,
    policy: ClaimPolicy,
) -> Result<PathBuf> {
    // Quick existence check (no lock needed).
    if !store.task_exists(id) {
        return Err(BallError::TaskNotFound(id.to_string()));
    }

    with_task_lock(store, id, || {
        // All validation happens under the lock so two claims on the
        // same task can't both pass.
        let mut task = store.load_task(id)?;
        if task.status != Status::Open {
            return Err(BallError::NotClaimable(format!(
                "{} (status = {})",
                id,
                task.status.as_str()
            )));
        }
        if task.claimed_by.is_some() {
            return Err(BallError::AlreadyClaimed(id.to_string()));
        }

        let all = store.all_tasks()?;
        if crate::ready::is_dep_blocked(&all, &task) {
            return Err(BallError::DepsUnmet(id.to_string()));
        }

        let wt_path = worktree_path(store, id)?;
        if wt_path.exists() {
            return Err(BallError::WorktreeExists(wt_path));
        }
        if claim_file_path(store, id).exists() {
            return Err(BallError::AlreadyClaimed(id.to_string()));
        }

        let branch = format!("work/{id}");
        task.status = Status::InProgress;
        task.claimed_by = Some(identity.to_string());
        task.branch = Some(branch.clone());
        task.touch();

        store.save_task(&task)?;
        store.commit_task(id, &format!("balls: claim {} - {}", id, task.title))?;

        if policy.require_remote && !store.stealth {
            sync_or_rollback(store, id, identity)?;
        }

        if let Some(parent) = wt_path.parent() {
            fs::create_dir_all(parent)?;
        }
        git::git_worktree_add(&store.root, &wt_path, &branch).inspect_err(|_| {
            let _ = rollback_claim(store, id);
        })?;

        link_shared_state(store, &wt_path)?;
        write_claim_file(store, id, identity)?;
        Ok(wt_path.clone())
    })
}

/// Push the freshly-committed claim through `origin/balls/tasks`. On
/// any failure — push rejected with our claim losing the merge,
/// remote unreachable, or other git error — roll the local claim back
/// so the task returns to `open` and surface a clear error.
fn sync_or_rollback(store: &Store, id: &str, identity: &str) -> Result<()> {
    match claim_sync::push_claim(store, id, identity) {
        Ok(claim_sync::SyncedClaimResult::Pushed) => Ok(()),
        Ok(claim_sync::SyncedClaimResult::Lost { winner }) => {
            // The merge already landed the winner's claim on disk; do
            // NOT rollback (that would clobber their state). Just fail.
            Err(BallError::AlreadyClaimed(format!("{id} (won by {winner})")))
        }
        Err(e) => {
            let _ = rollback_claim(store, id);
            Err(e)
        }
    }
}

fn link_shared_state(store: &Store, wt_path: &std::path::Path) -> Result<()> {
    let wt_balls = wt_path.join(".balls");
    fs::create_dir_all(&wt_balls)?;
    link_state_path(store.local_dir(), &wt_balls.join("local"))?;
    if !store.stealth {
        link_state_path(store.state_worktree_dir(), &wt_balls.join("worktree"))?;
        link_state_path(PathBuf::from("worktree/.balls/tasks"), &wt_balls.join("tasks"))?;
    }
    Ok(())
}

/// Symlink `src` -> `dst`. Mirror `store_init::ensure_tasks_symlink`:
/// idempotent on an existing symlink, but refuse to overwrite or
/// silently adopt any non-symlink entry that may have been planted at
/// `dst` between `git worktree add` and this call.
fn link_state_path(src: PathBuf, dst: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::symlink;
    if dst.is_symlink() {
        return Ok(());
    }
    if dst.exists() {
        return Err(BallError::Other(format!(
            "unexpected non-symlink at {}; refusing to link state into worktree",
            dst.display()
        )));
    }
    symlink(src, dst)?;
    Ok(())
}

fn rollback_claim(store: &Store, id: &str) -> Result<()> {
    if let Ok(mut t) = store.load_task(id) {
        t.status = Status::Open;
        t.claimed_by = None;
        t.branch = None;
        t.touch();
        store.save_task(&t)?;
        let _ = store.commit_task(id, &format!("balls: rollback claim {id}"));
    }
    let _ = fs::remove_file(claim_file_path(store, id));
    Ok(())
}

/// Claim without creating a git worktree: validate, flip status, write
/// the claim file. Used in no-git mode or when the caller explicitly
/// passes --no-worktree.
pub fn claim_no_worktree(
    store: &Store,
    id: &str,
    identity: &str,
    policy: ClaimPolicy,
) -> Result<()> {
    if !store.task_exists(id) {
        return Err(BallError::TaskNotFound(id.to_string()));
    }
    with_task_lock(store, id, || {
        let mut task = store.load_task(id)?;
        if task.status != Status::Open {
            return Err(BallError::NotClaimable(format!("{} (status = {})", id, task.status.as_str())));
        }
        if task.claimed_by.is_some() {
            return Err(BallError::AlreadyClaimed(id.to_string()));
        }
        let all = store.all_tasks()?;
        if crate::ready::is_dep_blocked(&all, &task) {
            return Err(BallError::DepsUnmet(id.to_string()));
        }
        task.status = Status::InProgress;
        task.claimed_by = Some(identity.to_string());
        task.touch();
        store.save_task(&task)?;
        store.commit_task(id, &format!("balls: claim {} - {}", id, task.title))?;
        if policy.require_remote && !store.stealth {
            sync_or_rollback(store, id, identity)?;
        }
        write_claim_file(store, id, identity)?;
        Ok(())
    })
}

pub fn drop_no_worktree(store: &Store, id: &str) -> Result<()> {
    with_task_lock(store, id, || {
        let mut t = store.load_task(id)?;
        let title = t.title.clone();
        t.status = Status::Open;
        t.claimed_by = None;
        t.branch = None;
        t.touch();
        store.save_task(&t)?;
        store.commit_task(id, &format!("balls: drop {id} - {title}"))?;
        let _ = fs::remove_file(claim_file_path(store, id));
        Ok(())
    })
}

pub fn drop_worktree(store: &Store, id: &str, force: bool) -> Result<()> {
    let wt_path = worktree_path(store, id)?;
    let task = store.load_task(id)?;
    let branch = task.branch.clone().unwrap_or_else(|| format!("work/{id}"));

    with_task_lock(store, id, || {
        if wt_path.exists() && !force && git::has_uncommitted_changes(&wt_path)? {
            return Err(BallError::Other(format!(
                "worktree {} has uncommitted changes. Use --force to drop.",
                wt_path.display()
            )));
        }

        let mut t = store.load_task(id)?;
        let title = t.title.clone();
        t.status = Status::Open;
        t.claimed_by = None;
        t.branch = None;
        t.touch();
        store.save_task(&t)?;
        store.commit_task(id, &format!("balls: drop {id} - {title}"))?;

        if wt_path.exists() {
            git::git_worktree_remove(&store.root, &wt_path, true)?;
        }
        let _ = git::git_branch_delete(&store.root, &branch, true);
        let _ = fs::remove_file(claim_file_path(store, id));
        Ok(())
    })
}

pub fn cleanup_orphans(store: &Store) -> Result<(Vec<String>, Vec<String>)> {
    let mut removed_claims = Vec::new();
    let mut removed_wts = Vec::new();
    let claims_dir = store.claims_dir();
    if claims_dir.exists() {
        for e in fs::read_dir(&claims_dir)? {
            let e = e?;
            let id = e.file_name().to_string_lossy().to_string();
            if !store.task_exists(&id) {
                let _ = fs::remove_file(e.path());
                removed_claims.push(id);
            }
        }
    }
    let wt_root = store.worktrees_root()?;
    if wt_root.exists() {
        for e in fs::read_dir(&wt_root)? {
            let e = e?;
            let id = e.file_name().to_string_lossy().to_string();
            if !claim_file_path(store, &id).exists() {
                let p = e.path();
                let _ = git::git_worktree_remove(&store.root, &p, true);
                removed_wts.push(id);
            }
        }
    }
    Ok((removed_claims, removed_wts))
}

#[cfg(test)]
#[path = "worktree_tests.rs"]
mod tests;
