//! Review, close, and archive — the submit side of the task lifecycle.
//! Lives alongside `worktree.rs` (claim/drop/orphans) but kept separate
//! so neither file hits the 300-line cap.

use crate::error::{BallError, Result};
use crate::store::Store;
use crate::task::{Status, Task};
use crate::worktree::{claim_file_path, with_task_lock, worktree_path};
use crate::{git, task_io};
use std::fs;

fn merge_or_fail(dir: &std::path::Path, branch: &str, ctx: &str) -> Result<()> {
    if let git::MergeResult::Conflict = git::git_merge(dir, branch)? {
        return Err(BallError::Conflict(ctx.to_string()));
    }
    Ok(())
}

/// Submit for review: commit the worker's code, squash-merge to main as
/// the single feature commit, flip task status to review on the state
/// branch. Keeps the worktree so a rejected review can be re-worked in
/// place.
pub fn review_worktree(
    store: &Store,
    id: &str,
    message: Option<&str>,
    identity: &str,
) -> Result<()> {
    let wt_path = worktree_path(store, id)?;
    let task = store.load_task(id)?;
    let branch = task.branch.clone().unwrap_or_else(|| format!("work/{}", id));

    with_task_lock(store, id, || {
        git::git_add_all(&wt_path)?;
        let _ = git::git_commit(&wt_path, &format!("wip: {}", id));
        let main_branch = git::git_current_branch(&store.root)?;
        merge_or_fail(
            &wt_path,
            &main_branch,
            &format!(
                "conflicts merging {} into work/{}. Resolve in worktree, then retry.",
                main_branch, id
            ),
        )?;

        // Squash merge the worker's branch into main. This is the single
        // substantive feature commit — the delivery tag [bl-XXXX] is
        // embedded in the title so tooling and humans can trace main
        // <-> state branch. The message is formatted in standard git
        // shape (title, blank, body) so `git log --oneline` stays
        // readable. Merge-in above already reconciled main into the
        // worktree, so this squash cannot itself produce fresh
        // conflicts.
        let squash_msg = crate::commit_msg::format_squash(message, &task.title, id);
        git::git_merge_squash(&store.root, &branch)?;
        git::git_commit(&store.root, &squash_msg)?;
        let delivered_sha = git::git_resolve_sha(&store.root, "HEAD")?;

        // Flip the task to review on the state branch, embedding the
        // delivery hint in the same commit so the state-branch history
        // stays at one-commit-per-transition.
        let task_path = store.task_path(id)?;
        let mut t = Task::load(&task_path)?;
        t.status = Status::Review;
        t.delivered_in = Some(delivered_sha);
        t.touch();
        t.save(&task_path)?;
        if let Some(msg) = message {
            task_io::append_note_to(&task_path, identity, msg)?;
        }
        store.commit_task(id, &format!("state: review {}", id))?;

        // Sync main back into worktree so re-review after rejection only
        // picks up new changes (squash merge doesn't record branch ancestry).
        let _ = git::git_merge(&wt_path, &main_branch);

        Ok(())
    })
}

/// Close a reviewed task: archive + remove worktree. Rejects from inside worktree.
pub fn close_worktree(
    store: &Store,
    id: &str,
    message: Option<&str>,
    identity: &str,
) -> Result<Task> {
    let wt_path = worktree_path(store, id)?;
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.starts_with(&wt_path) {
            return Err(BallError::Other(
                "cannot close from within the worktree — run from the repo root".into(),
            ));
        }
    }

    with_task_lock(store, id, || {
        let mut t = store.load_task(id)?;
        let branch = t.branch.clone().unwrap_or_else(|| format!("work/{}", id));
        t.status = Status::Closed;
        t.closed_at = Some(chrono::Utc::now());
        t.touch();

        if wt_path.exists() {
            git::git_worktree_remove(&store.root, &wt_path, true)?;
        }
        let _ = git::git_branch_delete(&store.root, &branch, true);
        let _ = fs::remove_file(claim_file_path(store, id));

        // close_and_archive is one atomic state-branch commit. The
        // reviewer's message is embedded in the commit body so it
        // survives the notes-file rm.
        let _ = identity;
        let msg = match message {
            Some(m) => format!("state: close {} - {}\n\n{}", id, t.title, m),
            None => format!("state: close {} - {}", id, t.title),
        };
        store.close_and_archive(&t, &msg)?;
        Ok(t)
    })
}
