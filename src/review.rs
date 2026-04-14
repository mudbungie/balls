//! Review, close, and archive — the submit side of the task lifecycle.
//! Lives alongside `worktree.rs` (claim/drop/orphans) but kept separate
//! so neither file hits the 300-line cap.

use crate::error::{BallError, Result};
use crate::store::Store;
use crate::task::{ArchivedChild, Status, Task};
use crate::worktree::{claim_file_path, with_task_lock, worktree_path};
use crate::{git, task_io};
use std::{fs, path::PathBuf};

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

        // Flip the task to review on the state branch. store.task_path
        // points at the state worktree's copy, not the bl worktree's.
        let task_path = store.task_path(id)?;
        let mut t = Task::load(&task_path)?;
        t.status = Status::Review;
        t.touch();
        t.save(&task_path)?;
        if let Some(msg) = message {
            task_io::append_note_to(&task_path, identity, msg)?;
        }
        store.commit_task(id, &format!("state: review {}", id))?;

        // Squash merge the worker's branch into main. This is the single
        // substantive feature commit — the delivery tag [bl-XXXX] is
        // embedded so tooling and humans can trace main <-> state branch.
        // Merge-in above already reconciled main into the worktree, so
        // this squash cannot itself produce fresh conflicts; if git
        // somehow leaves the index dirty, the commit below surfaces it.
        let squash_msg = match message {
            Some(msg) => format!("{} [{}]", msg, id),
            None => format!("{} [{}]", task.title, id),
        };
        git::git_merge_squash(&store.root, &branch)?;
        git::git_commit(&store.root, &squash_msg)?;

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
        store.save_task(&t)?;
        if let Some(msg) = message {
            let task_path = store.task_path(id)?;
            task_io::append_note_to(&task_path, identity, msg)?;
        }

        if wt_path.exists() {
            git::git_worktree_remove(&store.root, &wt_path, true)?;
        }
        let _ = git::git_branch_delete(&store.root, &branch, true);
        let _ = fs::remove_file(claim_file_path(store, id));

        // Archive stages deletions on the state branch; commit there, not main.
        archive_task(store, &t)?;
        store.commit_staged(&format!("state: close {} - {}", id, t.title))?;
        Ok(t)
    })
}

/// Stage task deletion and parent updates. Does NOT commit — caller is
/// responsible for committing all staged changes in one shot.
pub fn archive_task(store: &Store, task: &Task) -> Result<()> {
    let archived = ArchivedChild {
        id: task.id.clone(),
        title: task.title.clone(),
        closed_at: task.closed_at.unwrap_or_else(chrono::Utc::now),
    };

    // If this task has a parent, record the archived child on the parent
    if let Some(pid) = &task.parent {
        if let Ok(mut parent) = store.load_task(pid) {
            parent.closed_children.push(archived);
            parent.touch();
            store.save_task(&parent)?;
            if !store.stealth {
                let rel = PathBuf::from(format!(".balls/tasks/{}.json", pid));
                git::git_add(&store.state_worktree_dir(), &[rel.as_path()])?;
            }
        }
    }

    // Stage the git rm (non-stealth) or remove from fs (stealth).
    // The commit is issued by the caller via `commit_staged`.
    store.remove_task(&task.id)?;
    Ok(())
}
