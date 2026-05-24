//! State-branch push: the `git push <state_branch>` → `AttemptClass`
//! mapping (conflict vs. unreachable vs. other). Split out of
//! `claim_sync` so the `Protocol`/`Participant` wiring there stays
//! focused on the negotiation shape. Re-exported from `claim_sync`.

use crate::error::Result;
use crate::git;
use crate::negotiation::AttemptClass;
use std::path::Path;
use std::process::Output;

/// The state checkout's tracker remote. Under the unified model
/// `.balls/state-repo`'s `origin` *is* the tracker address, so every
/// state-branch push and fetch targets `origin` — there is no remote
/// name to resolve.
pub(crate) const STATE_REMOTE: &str = "origin";

/// Run `git push <state_remote> <branch>` from `dir` and classify the
/// outcome. Spawn failures propagate as Err — they're catastrophic
/// (no git on PATH) rather than a remote-state condition. The branch
/// is the clone's resolved tracker state branch (SPEC-tracker-state
/// §5), threaded in from the Store so a non-default `state_branch`
/// pushes to the same name it materialized.
pub fn push_state_classified(dir: &Path, remote: &str, branch: &str) -> Result<AttemptClass> {
    let out = git::clean_git_command(dir)
        .args(["push", remote, branch])
        .output()?;
    Ok(classify_push_output(&out))
}

fn classify_push_output(out: &Output) -> AttemptClass {
    if out.status.success() {
        return AttemptClass::Ok;
    }
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let l = stderr.to_lowercase();
    if l.contains("rejected")
        && (l.contains("non-fast-forward")
            || l.contains("fetch first")
            || l.contains("[rejected]"))
    {
        return AttemptClass::Conflict;
    }
    if is_unreachable(&l) {
        return AttemptClass::Unreachable(stderr);
    }
    AttemptClass::Other(stderr)
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

#[cfg(test)]
#[path = "claim_push_tests.rs"]
mod tests;
