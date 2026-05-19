//! Per-task target-branch push (bl-f788).
//!
//! bl-d4b0 let `bl review` land a task's squash on its own
//! `target_branch` rather than the repo-level integration branch.
//! `bl sync` historically pushed only that one repo-level branch, so a
//! per-task delivery was committed locally but never reached the code
//! remote — stranded. This module is the push side of the fix; the
//! read side (half-push's per-branch tag scan) lives in `half_push`.
//! Both consult the `target=<branch>` marker `bl review` writes on the
//! state-branch review subject.

use super::half_push::reviewed_target;
use balls::error::Result;
use balls::store::Store;
use balls::{git, git_state};
use std::path::Path;

/// Push the per-task target branches recorded in state-branch review
/// subjects that aren't the repo-level integration branch (already
/// pushed by the caller) and still have local commits the code remote
/// lacks. Best-effort per branch: a push failure warns and continues,
/// exactly as the repo-level main push is tolerated — the half-push
/// detector surfaces a stranded delivery on the next sync. A repo with
/// no per-task overrides records no `target=` markers, so the push set
/// is byte-identical to before this change.
pub(super) fn push_recorded_targets(store: &Store, remote: &str, repo_main: &str) -> Result<()> {
    let subjects = git_state::log_subjects(&store.state_worktree_dir(), "balls/tasks")?;
    let mut seen = std::collections::HashSet::new();
    for subj in &subjects {
        let Some((_, branch)) = reviewed_target(subj) else { continue };
        if branch == repo_main || !seen.insert(branch.clone()) {
            continue;
        }
        if !git_state::branch_exists(&store.root, &branch)
            || target_already_synced(&store.root, remote, &branch)
        {
            continue;
        }
        if let Err(e) = git::git_push(&store.root, remote, &branch) {
            eprintln!("warning: failed to push per-task target branch {branch}: {e}");
        }
    }
    Ok(())
}

/// True when `branch` is already at the code remote: its tip equals the
/// remote-tracking ref refreshed by the fetch at the top of
/// `sync_with_remote`. Lets the recorded-target push skip branches with
/// nothing new, bounding the push set to not-yet-synced deliveries.
fn target_already_synced(root: &Path, remote: &str, branch: &str) -> bool {
    if !git_state::has_remote_branch(root, remote, branch) {
        return false;
    }
    let local = git::git_resolve_sha(root, &format!("refs/heads/{branch}"));
    let tracked = git::git_resolve_sha(root, &format!("refs/remotes/{remote}/{branch}"));
    matches!((local, tracked), (Ok(l), Ok(r)) if l == r)
}
