//! Close paths split out of `review.rs` to keep that file under the
//! 300-line cap (bl-e454, when the `resolve_remote` thread pushed
//! `close_worktree` over). Shape preserved: `close_no_git` and
//! `close_worktree` are re-exported from `review` so callers see no
//! change. The gate-enforcement helpers, `review_*` paths, and the
//! `worktree`/`claim_sync` plumbing stay in `review.rs`.

use crate::claim_sync;
use crate::error::{BallError, Result};
use crate::participant::Event;
use crate::policy::ClaimPolicy;
use crate::review::enforce_gates;
use crate::store::Store;
use crate::task::{Status, Task};
use crate::worktree::{claim_file_path, with_task_lock, worktree_path};
use crate::{git, task_io};
use std::fs;

/// Close in no-git mode: archive task, no worktree teardown.
pub fn close_no_git(store: &Store, id: &str, message: Option<&str>, identity: &str) -> Result<Task> {
    with_task_lock(store, id, || {
        let mut t = store.load_task(id)?;
        enforce_gates(store, &t)?;
        t.status = Status::Closed;
        t.closed_at = Some(chrono::Utc::now());
        t.touch();
        let _ = fs::remove_file(claim_file_path(store, id));
        let msg = match message {
            Some(m) => format!("state: close {} - {}\n\n{}", id, t.title, m),
            None => format!("state: close {} - {}", id, t.title),
        };
        if let Some(note) = message {
            let task_path = store.task_path(id)?;
            task_io::append_note_to(&task_path, identity, note)?;
        }
        store.close_and_archive(&t, &msg)?;
        Ok(t)
    })
}

/// Close a reviewed task: archive + remove worktree. Rejects from
/// inside worktree. When `policy.require_remote` is set the
/// state-branch close commit is pushed via the git-remote participant
/// before the destructive worktree teardown; a Required-policy failure
/// rolls the state-branch commit back so the task file (and worktree)
/// are still in place for a retry. Order matters: the push must
/// happen *before* `git_worktree_remove`, so a rejected push doesn't
/// leave the user with a vanished worktree they cannot resume from.
#[allow(clippy::too_many_arguments)]
pub fn close_worktree(
    store: &Store,
    id: &str,
    message: Option<&str>,
    identity: &str,
    policy: ClaimPolicy,
    delivered: Option<String>,
    delivered_repo: Option<String>,
    resolve_remote: bool,
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
        enforce_gates(store, &t)?;
        // bl-87ea: deferred mode never wrote `delivered_in` (no local
        // squash) and `--delivered` overrides the scan. Commit a newly
        // resolved hint to the state branch *before* `close_and_archive`
        // git-rm's the file, so archive recovery's pre-deletion blob
        // carries it — as local-squash mode persists it in `review`.
        let cfg = store.load_config()?;
        let target = cfg.integration_branch_for(&store.root, t.target_branch.as_deref())?;
        if crate::delivery::populate_on_close(
            &store.root,
            &target,
            &mut t,
            delivered,
            delivered_repo,
            resolve_remote,
        ) {
            store.save_task(&t)?;
            store.commit_task(id, &format!("state: deliver {id}"))?;
        }
        let branch = t.branch.clone().unwrap_or_else(|| format!("work/{id}"));
        t.status = Status::Closed;
        t.closed_at = Some(chrono::Utc::now());
        t.touch();

        // close_and_archive is one atomic state-branch commit. The
        // reviewer's message is embedded in the commit body so it
        // survives the notes-file rm.
        let _ = identity;
        let msg = match message {
            Some(m) => format!("state: close {} - {}\n\n{}", id, t.title, m),
            None => format!("state: close {} - {}", id, t.title),
        };
        // bl-2bf7: snapshot pre-close state-branch tip so a Required
        // failure on the push can roll back close_and_archive's commit
        // (which removed the task file) and keep the worktree intact.
        // Captured before `close_and_archive` to avoid the same
        // HEAD~1 race that bites review's path under concurrent state
        // advances.
        let state_dir = store.state_repo_dir();
        let pre_state_sha =
            (policy.require_remote && !store.stealth)
                .then(|| git::git_resolve_sha(&state_dir, "HEAD"))
                .transpose()?;
        store.close_and_archive(&t, &msg)?;

        if let Some(pre_state) = pre_state_sha.as_deref() {
            if let Err(e) = claim_sync::push_state_for(
                store,
                id,
                identity,
                Event::Close,
                "close --sync",
            ) {
                let _ = git::git_reset_hard(&state_dir, pre_state);
                return Err(e);
            }
        }

        if wt_path.exists() {
            git::git_worktree_remove(&store.root, &wt_path, true)?;
        }
        let _ = git::git_branch_delete(&store.root, &branch, true);
        let _ = fs::remove_file(claim_file_path(store, id));
        Ok(t)
    })
}
