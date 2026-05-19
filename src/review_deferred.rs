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

use crate::error::Result;
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
