//! Teardown paths split out of `worktree.rs`: releasing a claim
//! (`drop_no_worktree` / `drop_worktree`) and sweeping stale claim
//! files + dangling worktrees (`cleanup_orphans`). The claim/create
//! side stays in `worktree.rs`; both share only the lock and
//! path helpers, re-exported here from `worktree`.

use crate::error::{BallError, Result};
use crate::git;
use crate::store::Store;
use crate::task::Status;
use crate::worktree::{claim_file_path, with_task_lock, worktree_path};
use std::fs;

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
