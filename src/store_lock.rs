//! flock-based serialization primitives, split out of `store.rs`.
//!
//! Two locks live here: the per-task lock (`task_lock`, public — the
//! lifecycle/command paths take it around a task mutation) and the
//! store-wide state-worktree lock (`state_worktree_flock`, crate-only
//! — held across any state-branch git write so `index.lock` never
//! sees contention). Re-exported from `store` so the public API path
//! `balls::store::{task_lock, LockGuard}` stays stable.

use crate::error::Result;
use crate::store::Store;
use crate::task;
use fs2::FileExt;
use std::fs;
use std::path::Path;

/// Acquire an exclusive flock on the given path. The lock is released when
/// the returned guard is dropped.
pub struct LockGuard(fs::File);
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub fn task_lock(store: &Store, id: &str) -> Result<LockGuard> {
    task::validate_id(id)?;
    acquire_flock(&store.lock_dir().join(format!("{id}.lock")))
}

/// Acquire the store-wide state-worktree lock. Held for the duration
/// of any write sequence targeting the state branch (commit_task,
/// commit_staged, remove_task, close_and_archive). Serializes
/// concurrent bl invocations from different tasks so git's
/// `index.lock` never sees contention.
pub(crate) fn state_worktree_flock(store: &Store) -> Result<LockGuard> {
    acquire_flock(&store.lock_dir().join("state-worktree.lock"))
}

fn acquire_flock(path: &Path) -> Result<LockGuard> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    f.lock_exclusive()?;
    Ok(LockGuard(f))
}
