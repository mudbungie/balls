//! Deferred-mode `bl review` (SPEC §7.2).
//!
//! Where local-squash mode lands the squash on the integration branch
//! immediately, deferred mode hands that off to an external forge. The
//! work branch is pushed to the code remote and an auto-gate child task
//! is opened; the parent flips to `review` but the integration branch
//! is left untouched and `delivered_in` stays null until the forge
//! produces the merge commit.
//!
//! The auto-gate is the backwards-compatibility hinge: the existing
//! `gates` close-blocker (enforced since before this spec) means even
//! an old `bl` that never heard of deferred mode refuses to tear the
//! parent down mid-review. No new lifecycle state, no new old-client
//! code — a reused primitive (SPEC §3, §4).

use crate::error::{BallError, Result};
use crate::git;
use crate::store::Store;
use crate::task::{Link, LinkType, NewTaskOpts, Status, Task, TaskType};
use crate::{commit_msg, task_id, task_io};
use std::collections::BTreeMap;
use std::path::Path;

/// Push the work branch to the code remote, open the forge-gate child,
/// link the parent `gates → child`, and flip the parent to `review`.
/// Does **not** squash into `target_branch` and does **not** set
/// `delivered_in` — both happen later, when the forge merges the PR.
/// Called from `review::review_worktree` with the worktree already
/// committed and merged up to `target_branch` (conflicts have already
/// surfaced), under the parent's task lock.
#[allow(clippy::too_many_arguments)]
pub fn deferred_review(
    store: &Store,
    wt: &Path,
    id: &str,
    branch: &str,
    task: &Task,
    target_branch: &str,
    message: Option<&str>,
    identity: &str,
) -> Result<()> {
    // 1. Publish the work branch on the code remote. A push failure
    //    aborts review with the worktree intact, exactly like a merge
    //    conflict would — retry after fixing remote access.
    git::git_push(wt, "origin", branch)?;

    // 2. Open the forge-gate child. It carries the `forge-gate` tag so
    //    it is filterable, and inherits the parent's repo provenance so
    //    it reads correctly on a shared hub. It is deliberately left
    //    `open` and unclaimed: closing it is the forge plugin's job (or
    //    a human's, post-merge). SKILL.md's "don't claim a gate target"
    //    guidance keeps agents off it — no enforcement code needed.
    let title = format!("Forge: PR merged for {id}");
    let child_id = task_id::generate_task_id(store, &title)?;
    let mut child = Task::new(
        NewTaskOpts {
            title: title.clone(),
            task_type: TaskType::task(),
            priority: 3,
            parent: Some(id.to_string()),
            depends_on: Vec::new(),
            description: format!(
                "Auto-opened by deferred-mode `bl review`. The forge PR for {id} \
                 must merge before this closes; closing it (plugin sync, or a human \
                 after merging) unblocks `bl close {id}`. Do not claim — it gates {id}."
            ),
            tags: vec!["forge-gate".to_string()],
        },
        child_id.clone(),
    );
    child.repo.clone_from(&task.repo);
    store.save_task(&child)?;
    store.commit_task(&child_id, &format!("balls: create {child_id} - {title}"))?;

    // 3. Gate the parent on the child and flip it to `review` in one
    //    state-branch commit. Order matters: the gate link must exist
    //    before (or with) the status flip so an old `bl close` racing
    //    right after the flip already sees the open gate.
    let mut parent = store.load_task(id)?;
    let link = Link {
        link_type: LinkType::Gates,
        target: child_id.clone(),
        extra: BTreeMap::new(),
    };
    if !parent.links.contains(&link) {
        parent.links.push(link);
    }
    parent.status = Status::Review;
    parent.touch();
    store.save_task(&parent)?;
    if let Some(msg) = message {
        task_io::append_note_to(&store.task_path(id)?, identity, msg)?;
    }
    store.commit_task(id, &format!("state: review {id} deferred"))?;

    // 4. Tell the operator what to do next: open the PR with the
    //    delivery tag preserved, and close the gate when it merges.
    let pr_title = commit_msg::subject_with_tag(message, &task.title, id);
    println!("deferred: pushed {branch} to origin (base {target_branch})");
    println!("PR title: {pr_title}");
    println!(
        "gate task: {child_id} — close it when the PR merges to unblock `bl close {id}`"
    );
    eprintln!(
        "{id} is in review and gated by {child_id}; `bl close {id}` is blocked until the PR merges."
    );
    Ok(())
}

/// SPEC §7.3 — reject a deferred-mode review. Flip the parent back to
/// `in_progress` *and* close its open `forge-gate` child in the SAME
/// state-branch commit, dropping the now-dead `gates` link, so the
/// invariant holds: a task is `in_progress` iff it has no open gate
/// child. Returns `Ok(false)` when there is no such child — the caller
/// then does a plain status update, so the non-deferred reject path is
/// unchanged. Refuses without mutation if the gate child is claimed:
/// archiving a claimed task needs the worktree teardown the update
/// path does not own, so the operator resolves that first. The work
/// branch on `origin` is deliberately left alone — the operator or
/// forge plugin closes the PR if the work is being abandoned.
///
/// `parent` arrives already mutated to `status=in_progress` by the
/// caller's field-apply loop; this owns the persist (save + the atomic
/// archive commit) when it handles the reject.
pub fn reject_deferred(
    store: &Store,
    parent: &mut Task,
    note: Option<&str>,
    identity: &str,
) -> Result<bool> {
    // `open_gate_blockers` returns every still-open gate-linked child
    // and already absorbs archived targets / load errors (covered in
    // `review`). Of those, only the deferred-mode auto-gate carries
    // the `forge-gate` tag (SPEC §7.2 step 4); a hand-added audit gate
    // is deliberate and stays put.
    let blockers = crate::review::open_gate_blockers(store, parent)?;
    let mut gate = None;
    for bid in &blockers {
        let c = store.load_task(bid)?;
        if c.tags.iter().any(|t| t == "forge-gate") {
            gate = Some(c);
            break;
        }
    }
    let Some(mut child) = gate else {
        return Ok(false);
    };
    // Atomic refusal: a claimed gate child can't be torn down here
    // (that needs `bl close`'s worktree teardown). Bail before any
    // mutation so the reject is observably all-or-nothing.
    if child.claimed_by.is_some() {
        return Err(BallError::Other(format!(
            "cannot reject {}: gate child {} is claimed — close or drop it first",
            parent.id, child.id
        )));
    }
    parent
        .links
        .retain(|l| !(matches!(l.link_type, LinkType::Gates) && l.target == child.id));
    parent.touch();
    store.save_task(parent)?;
    if let Some(n) = note {
        task_io::append_note_to(&store.task_path(&parent.id)?, identity, n)?;
    }
    child.status = Status::Closed;
    child.closed_at = Some(chrono::Utc::now());
    child.touch();
    // The reject note lives in the parent's (un-archived) notes file,
    // so — unlike a close — it need not be embedded in the commit
    // message; mirror the plain `balls: update` single-line shape.
    let msg = format!(
        "state: reject {} deferred — closed gate {}",
        parent.id, child.id
    );
    // `close_and_archive` re-reads the parent — picking up the status
    // flip and dropped link we just saved — appends the closed-child
    // record, and stages parent (json + notes) plus the child removal
    // into ONE commit. That single commit is the atomicity SPEC §7.3
    // requires.
    store.close_and_archive(&child, &msg)?;
    Ok(true)
}
