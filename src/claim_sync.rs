//! Push-retry-resolve loop for the claim path. Closes the claim-race
//! window between offline agents by requiring the local claim commit
//! on `balls/tasks` to land on the remote before the worktree is
//! created. Race resolution: git's native CAS (non-fast-forward push
//! rejection) drives a fetch + `resolve_conflict()` merge; whichever
//! agent's `claimed_by` survives the field-level merge wins.
//!
//! Wired in only when `ClaimPolicy::require_remote` is true. With it
//! off, claims stay local-only — preserving the offline-and-solo
//! workflow.

use crate::error::{BallError, Result};
use crate::git;
use crate::store::Store;
use std::path::Path;
use std::process::Output;

const REMOTE: &str = "origin";
const STATE_BRANCH: &str = "balls/tasks";
const MAX_RETRIES: usize = 5;

/// Outcome of a single push attempt against the state branch.
#[derive(Debug, PartialEq, Eq)]
pub enum PushClass {
    Ok,
    Rejected,
    Unreachable(String),
    Other(String),
}

/// Run `git push origin balls/tasks` from `dir` and classify the
/// outcome. Spawn failures propagate as Err — they're catastrophic
/// (no git on PATH) rather than a remote-state condition.
pub fn push_state_classified(dir: &Path) -> Result<PushClass> {
    let out = git::clean_git_command(dir)
        .args(["push", REMOTE, STATE_BRANCH])
        .output()?;
    Ok(classify_push_output(&out))
}

fn classify_push_output(out: &Output) -> PushClass {
    if out.status.success() {
        return PushClass::Ok;
    }
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let l = stderr.to_lowercase();
    if l.contains("rejected")
        && (l.contains("non-fast-forward")
            || l.contains("fetch first")
            || l.contains("[rejected]"))
    {
        return PushClass::Rejected;
    }
    if is_unreachable(&l) {
        return PushClass::Unreachable(stderr);
    }
    PushClass::Other(stderr)
}

fn is_unreachable(stderr_lower: &str) -> bool {
    const UNREACHABLE_MARKERS: &[&str] = &[
        "could not resolve",
        "could not read from remote",
        "connection refused",
        "connection timed out",
        "connection reset",
        "repository not found",
        "permission denied",
        "does not appear to be a git repository",
        "unable to access",
        "host key verification failed",
        "no such host",
        "network is unreachable",
    ];
    UNREACHABLE_MARKERS.iter().any(|m| stderr_lower.contains(m))
}

/// Outcome of the push-with-retry-resolve loop.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncedClaimResult {
    /// Our claim landed on the remote.
    Pushed,
    /// Another agent got there first; their claim is now reflected in
    /// the local task file (via the auto-resolved merge). The caller
    /// must NOT create a worktree.
    Lost { winner: String },
}

/// Pure loop: drive a push function, resolving non-FF rejections via
/// the supplied `merge_resolve` step and checking `load_claimer` to
/// detect a lost claim. Extracted from `push_claim` so the
/// state-machine can be unit-tested with synthetic closures —
/// real-git integration tests cover the wiring separately.
pub fn run_push_loop<P, M, L>(
    identity: &str,
    max_retries: usize,
    mut push: P,
    mut merge_resolve: M,
    mut load_claimer: L,
) -> Result<SyncedClaimResult>
where
    P: FnMut() -> Result<PushClass>,
    M: FnMut() -> Result<()>,
    L: FnMut() -> Result<Option<String>>,
{
    for _ in 0..max_retries {
        let class = push()?;
        if class == PushClass::Ok {
            return Ok(SyncedClaimResult::Pushed);
        }
        if let PushClass::Unreachable(s) | PushClass::Other(s) = class {
            return Err(BallError::Other(format!(
                "claim --sync: push `{STATE_BRANCH}` failed: {s}"
            )));
        }
        // Rejected: fetch + merge + check who won the field-level merge.
        merge_resolve()?;
        let claimer = load_claimer()?;
        if claimer.as_deref() != Some(identity) {
            let winner = claimer.unwrap_or_else(|| "(unknown)".into());
            let _ = push();
            return Ok(SyncedClaimResult::Lost { winner });
        }
    }
    Err(BallError::Other(format!(
        "claim --sync: gave up after {max_retries} push attempts; remote keeps advancing"
    )))
}

/// Push the freshly-committed claim through `origin/balls/tasks`.
/// Caller has already (a) committed the claim locally on the state
/// branch and (b) released no locks that the merge step needs.
pub fn push_claim(
    store: &Store,
    task_id: &str,
    identity: &str,
) -> Result<SyncedClaimResult> {
    let state_dir = store.state_worktree_dir();
    if !git::git_fetch(&state_dir, REMOTE)? {
        return Err(BallError::Other(format!(
            "claim --sync: cannot reach remote `{REMOTE}` (fetch failed)"
        )));
    }
    let push = || push_state_classified(&state_dir);
    let merge_resolve = || {
        let _ = git::git_fetch(&state_dir, REMOTE);
        let merge = git::git_merge(&state_dir, &format!("{REMOTE}/{STATE_BRANCH}"))?;
        if matches!(merge, git::MergeResult::Conflict) {
            crate::sync_resolve::auto_resolve_task_conflicts(&state_dir)?;
            git::git_commit(&state_dir, "state: auto-resolve claim conflicts")?;
        }
        Ok(())
    };
    let load_claimer = || Ok(store.load_task(task_id)?.claimed_by);
    run_push_loop(identity, MAX_RETRIES, push, merge_resolve, load_claimer)
}

#[cfg(test)]
#[path = "claim_sync_tests.rs"]
mod tests;
