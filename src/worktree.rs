//! Worktree scaffolding: claim, drop, and orphan cleanup. The submit
//! side (review, close, archive) lives in `review.rs` to keep both
//! files under the 300-line cap.

use crate::error::{BallError, Result};
use crate::store::{task_lock, Store};
use crate::task::{self, Status};
use crate::git;
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

pub fn create_worktree(store: &Store, id: &str, identity: &str) -> Result<PathBuf> {
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

        let branch = format!("work/{}", id);
        task.status = Status::InProgress;
        task.claimed_by = Some(identity.to_string());
        task.branch = Some(branch.clone());
        task.touch();

        store.save_task(&task)?;
        store.commit_task(id, &format!("balls: claim {} - {}", id, task.title))?;

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

fn link_shared_state(store: &Store, wt_path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::symlink;
    let wt_balls = wt_path.join(".balls");
    fs::create_dir_all(&wt_balls)?;
    let link = |src: PathBuf, name: &str| -> Result<()> {
        let dst = wt_balls.join(name);
        if !dst.exists() {
            symlink(src, dst)?;
        }
        Ok(())
    };
    link(store.local_dir(), "local")?;
    if !store.stealth {
        link(store.state_worktree_dir(), "worktree")?;
        link(PathBuf::from("worktree/.balls/tasks"), "tasks")?;
    }
    Ok(())
}

fn rollback_claim(store: &Store, id: &str) -> Result<()> {
    if let Ok(mut t) = store.load_task(id) {
        t.status = Status::Open;
        t.claimed_by = None;
        t.branch = None;
        t.touch();
        store.save_task(&t)?;
        let _ = store.commit_task(id, &format!("balls: rollback claim {}", id));
    }
    let _ = fs::remove_file(claim_file_path(store, id));
    Ok(())
}

pub fn drop_worktree(store: &Store, id: &str, force: bool) -> Result<()> {
    let wt_path = worktree_path(store, id)?;
    let task = store.load_task(id)?;
    let branch = task.branch.clone().unwrap_or_else(|| format!("work/{}", id));

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
        store.commit_task(id, &format!("balls: drop {} - {}", id, title))?;

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
