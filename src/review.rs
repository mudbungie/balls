//! Review, close, and archive — the submit side of the task lifecycle.
//! Lives alongside `worktree.rs` (claim/drop/orphans) but kept separate
//! so neither file hits the 300-line cap. The close paths
//! (`close_no_git` and `close_worktree`) moved to `review_close.rs`
//! when bl-e454 pushed the file over the cap; both are re-exported
//! below so `balls::review::close_*` callers stay byte-identical.

use crate::claim_sync;
use crate::error::{BallError, Result};
use crate::link::LinkType;
use crate::participant::Event;
use crate::policy::ClaimPolicy;
use crate::store::Store;
use crate::task::{Status, Task};
use crate::worktree::{with_task_lock, worktree_path};
use crate::{git, task_io};

pub use crate::review_close::{close_no_git, close_worktree};

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

/// Run the configured `review.pre_check` gate (bl-1f38) in the worktree.
/// `bl review` calls this once the worker's work is committed and the
/// integration branch is merged in, so the check sees the exact
/// end-state being delivered — and *before* the squash or branch push,
/// so a non-zero exit aborts the review with nothing to roll back:
/// status stays `in_progress`, the integration branch is untouched. The
/// check inherits stdio, so its own output streams straight to the
/// terminal. `None` ⇒ no gate configured, the historical behavior.
fn run_pre_check(cmd: Option<&str>, dir: &std::path::Path) -> Result<()> {
    let Some(cmd) = cmd else { return Ok(()) };
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(dir)
        .status()?;
    if status.success() {
        return Ok(());
    }
    Err(BallError::Other(format!(
        "review pre-check failed: `{cmd}` exited non-zero. No squash, no \
         status change — fix it in the worktree and retry `bl review`."
    )))
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
        let cfg = store.load_config()?;
        let deferred = matches!(cfg.delivery_mode(), crate::config::DeliveryMode::Deferred);
        // SPEC §5: deferred mode hands the squash to a forge, so the PR
        // base must be unambiguous — an implicit "whatever's checked
        // out" target is rejected. Fail before any mutation. Per SPEC
        // §6.7, the resolution chain is `task.target_branch ??
        // config.target_branch`; either being set satisfies the gate.
        if deferred && task.target_branch.is_none() && cfg.target_branch.is_none() {
            return Err(BallError::Other(
                "delivery.mode=deferred requires an explicit target_branch \
                 (per-task via `bl create --target-branch`, or repo-level \
                 under the legacy layout) — the forge PR base must be \
                 unambiguous"
                    .into(),
            ));
        }
        crate::review_safety::add_user_changes(&wt_path)?;
        let _ = git::git_commit(&wt_path, &format!("wip: {id}"));
        let main_branch =
            cfg.integration_branch_for(&store.root, task.target_branch.as_deref())?;
        merge_or_fail(
            &wt_path,
            &main_branch,
            &format!(
                "conflicts merging {main_branch} into work/{id}. Resolve in worktree, then retry."
            ),
        )?;

        // The quality gate. Runs against the post-merge worktree —
        // exactly the end-state about to be delivered — and before both
        // the deferred-mode push and the local squash, so a failure
        // never lands code on the integration branch (bl-1f38).
        run_pre_check(cfg.review_pre_check(), &wt_path)?;

        if deferred {
            return crate::review_deferred::deferred_review(
                store, &wt_path, id, &branch, &task, &main_branch, message, identity,
            );
        }

        // Snapshot pre-review tips so any failure between the squash
        // commit and the state-branch flip (or the require_remote
        // push) can rewind main back to where it was. Without this
        // rewind, `bl review` can return failure while having already
        // mutated the integration branch — see bl-0dc3 for the repro.
        // The state-branch snapshot must be taken before `commit_task`
        // lands the review commit; another agent advancing the state
        // branch concurrently would otherwise make `HEAD~1` ambiguous.
        // Snapshot the integration branch by name, not `HEAD`: with a
        // configured `target_branch`, HEAD at the root is *not* the
        // branch the squash lands on. For the default (target unset,
        // integration branch == checkout) `rev-parse <branch>` and
        // `rev-parse HEAD` resolve the same sha — byte-identical.
        let pre_main_sha = git::git_resolve_sha(&store.root, &main_branch)?;
        let state_dir = store.state_repo_dir();
        let pre_state_sha =
            (policy.require_remote && !store.stealth)
                .then(|| git::git_resolve_sha(&state_dir, "HEAD"))
                .transpose()?;

        let squash_msg = crate::commit_msg::format_squash(message, &task.title, id);
        let transition = (|| -> Result<()> {
            crate::review_safety::commit_squash_and_flip(
                store,
                id,
                &branch,
                &squash_msg,
                message,
                identity,
                &pre_main_sha,
                &main_branch,
            )?;
            if let Some(pre_state) = pre_state_sha.as_deref() {
                claim_sync::push_state_for(store, id, identity, Event::Review, "review --sync")
                    .inspect_err(|_| {
                        let _ = git::git_reset_hard(&state_dir, pre_state);
                    })?;
            }
            Ok(())
        })();
        if let Err(e) = transition {
            let _ = crate::review_safety::rewind_main(store, &main_branch, &pre_main_sha);
            return Err(e);
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

// `close_no_git` and `close_worktree` live in `review_close.rs` and
// are re-exported from `lib.rs` so callers see no change.

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
