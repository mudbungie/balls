//! Review, close, and archive — the submit side of the task lifecycle.
//! Lives alongside `worktree.rs` (claim/drop/orphans) but kept separate
//! so neither file hits the 300-line cap.

use crate::claim_sync;
use crate::error::{BallError, Result};
use crate::link::LinkType;
use crate::participant::Event;
use crate::policy::ClaimPolicy;
use crate::store::Store;
use crate::task::{Status, Task};
use crate::worktree::{claim_file_path, with_task_lock, worktree_path};
use crate::{git, task_io};
use std::fs;

/// Return the IDs of any `gates`-linked children of `parent` that are
/// still open in the store. A child is "open" if its task file is still
/// present — once closed, `close_and_archive` removes it. Callers that
/// are about to close `parent` must reject the close if this list is
/// non-empty.
pub fn open_gate_blockers(store: &Store, parent: &Task) -> Result<Vec<String>> {
    let mut blockers = Vec::new();
    for link in &parent.links {
        if !matches!(link.link_type, LinkType::Gates) {
            continue;
        }
        // load_task returns TaskNotFound if the child is already
        // archived (i.e. the gate has been satisfied). Any other
        // error bubbles up — we don't want to silently skip a
        // malformed gate child and let the parent close.
        match store.load_task(&link.target) {
            Ok(child) => {
                if child.status != Status::Closed {
                    blockers.push(link.target.clone());
                }
            }
            Err(BallError::TaskNotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(blockers)
}

/// Build the error returned when a close is blocked by open gates.
/// Factored out so both close paths produce the same message.
fn gate_blocked_error(parent_id: &str, blockers: &[String]) -> BallError {
    BallError::Other(format!(
        "cannot close {parent_id}: blocked by open gate{plural} {list}. \
         Close the gate task{plural} first, or run `bl link rm {parent_id} gates <id>` to drop a gate.",
        plural = if blockers.len() == 1 { "" } else { "s" },
        list = blockers.join(", "),
    ))
}

/// Public wrapper: check gates and return a ready-to-return error if
/// any are open. Both close paths share this to keep the message and
/// semantics aligned.
pub fn enforce_gates(store: &Store, parent: &Task) -> Result<()> {
    let blockers = open_gate_blockers(store, parent)?;
    if !blockers.is_empty() {
        return Err(gate_blocked_error(&parent.id, &blockers));
    }
    Ok(())
}

fn merge_or_fail(dir: &std::path::Path, branch: &str, ctx: &str) -> Result<()> {
    if let git::MergeResult::Conflict = git::git_merge(dir, branch)? {
        return Err(BallError::Conflict(ctx.to_string()));
    }
    Ok(())
}

/// Submit for review: commit the worker's code, squash-merge to main as
/// the single feature commit, flip task status to review on the state
/// branch. Keeps the worktree so a rejected review can be re-worked in
/// place. When `policy.require_remote` is set the state-branch review
/// commit is pushed via the git-remote participant; a Required-policy
/// failure rolls back both the state-branch commit and the squash on
/// main so the transition is observably atomic per bl-2bf7.
pub fn review_worktree(
    store: &Store,
    id: &str,
    message: Option<&str>,
    identity: &str,
    policy: ClaimPolicy,
) -> Result<()> {
    let wt_path = worktree_path(store, id)?;
    let task = store.load_task(id)?;
    let branch = task.branch.clone().unwrap_or_else(|| format!("work/{id}"));

    with_task_lock(store, id, || {
        git::git_add_all(&wt_path)?;
        let _ = git::git_commit(&wt_path, &format!("wip: {id}"));
        let main_branch = git::git_current_branch(&store.root)?;
        merge_or_fail(
            &wt_path,
            &main_branch,
            &format!(
                "conflicts merging {main_branch} into work/{id}. Resolve in worktree, then retry."
            ),
        )?;

        // Snapshot the pre-review main and state-branch tips so a
        // required-participant rejection can roll the working trees
        // back to their pre-transition state. The state-branch
        // snapshot must be taken before `commit_task` lands the
        // review commit; another agent advancing the state branch
        // concurrently would otherwise make `HEAD~1` ambiguous.
        let pre_main_sha = git::git_resolve_sha(&store.root, "HEAD")?;
        let state_dir = store.state_worktree_dir();
        let pre_state_sha =
            (policy.require_remote && !store.stealth)
                .then(|| git::git_resolve_sha(&state_dir, "HEAD"))
                .transpose()?;

        // Squash merge the worker's branch into main as the single
        // feature commit; the delivery tag [bl-XXXX] is embedded in
        // the title so main <-> state branch stays traceable. The
        // merge-in above already reconciled main, so this squash
        // cannot produce fresh conflicts. An empty squash (no staged
        // changes after `merge --squash`) means the worker delivered
        // no code — e.g. a gate task or a bugfix that turned no-op
        // once main moved forward; we skip the commit so
        // delivered_in stays null and the state-branch subject gets
        // a `no-code` marker for half-push detection.
        let squash_msg = crate::commit_msg::format_squash(message, &task.title, id);
        git::git_merge_squash(&store.root, &branch)?;
        let delivered_sha = if git::has_staged_changes(&store.root)? {
            git::git_commit(&store.root, &squash_msg)?;
            Some(git::git_resolve_sha(&store.root, "HEAD")?)
        } else {
            eprintln!("no code delivered — checkpoint review for {id}");
            None
        };

        // Flip the task to review on the state branch, embedding the
        // delivery hint in the same commit so the state-branch history
        // stays at one-commit-per-transition.
        let had_delivery = delivered_sha.is_some();
        let task_path = store.task_path(id)?;
        let mut t = Task::load(&task_path)?;
        t.status = Status::Review;
        t.delivered_in = delivered_sha;
        t.touch();
        t.save(&task_path)?;
        if let Some(msg) = message {
            task_io::append_note_to(&task_path, identity, msg)?;
        }
        let state_msg = if had_delivery {
            format!("state: review {id}")
        } else {
            format!("state: review {id} no-code")
        };
        store.commit_task(id, &state_msg)?;

        // bl-2bf7: when require_remote is on, push the state-branch
        // review commit through the git-remote participant. A Required
        // failure rolls main and the state worktree back to their
        // pre-review SHAs so the transition is observably atomic.
        if let Some(pre_state) = pre_state_sha.as_deref() {
            if let Err(e) = claim_sync::push_state_for(
                store,
                id,
                identity,
                Event::Review,
                "review --sync",
            ) {
                let _ = git::git_reset_hard(&state_dir, pre_state);
                let _ = git::git_reset_hard(&store.root, &pre_main_sha);
                return Err(e);
            }
        }

        // Sync main back into worktree so re-review after rejection only
        // picks up new changes (squash merge doesn't record branch ancestry).
        let _ = git::git_merge(&wt_path, &main_branch);

        Ok(())
    })
}

/// Review in no-git mode: flip status, no squash merge.
pub fn review_no_git(store: &Store, id: &str, message: Option<&str>, identity: &str) -> Result<()> {
    with_task_lock(store, id, || {
        let task_path = store.task_path(id)?;
        let mut t = Task::load(&task_path)?;
        t.status = Status::Review;
        t.touch();
        t.save(&task_path)?;
        if let Some(msg) = message {
            task_io::append_note_to(&task_path, identity, msg)?;
        }
        store.commit_task(id, &format!("state: review {id}"))?;
        Ok(())
    })
}

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
pub fn close_worktree(
    store: &Store,
    id: &str,
    message: Option<&str>,
    identity: &str,
    policy: ClaimPolicy,
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
        let state_dir = store.state_worktree_dir();
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
