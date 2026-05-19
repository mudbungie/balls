//! State-branch push: remote resolution and outcome classification.
//!
//! Split out of `claim_sync` so the `Protocol`/`Participant` wiring
//! there stays focused on the negotiation shape. This module owns the
//! one seam that resolves the effective `state_remote` and the
//! `git push balls/tasks` → `AttemptClass` mapping (conflict vs.
//! unreachable vs. other). Re-exported from `claim_sync`.

use crate::error::Result;
use crate::git;
use crate::negotiation::AttemptClass;
use crate::store::Store;
use std::path::Path;
use std::process::Output;

pub(crate) const STATE_BRANCH: &str = "balls/tasks";

/// Resolve this repo's effective `state_remote` — the one seam that
/// retargets `balls/tasks` (per-clone override over committed,
/// default `origin`). No other code path bakes in `origin`. Errors
/// propagate: a required sync must not silently hit the wrong peer.
pub(crate) fn state_remote(store: &Store) -> Result<String> {
    let cfg = store.load_config()?;
    let local = crate::policy::LocalConfig::load(store)?;
    Ok(crate::policy::state_remote_opt(&cfg, local.as_ref())
        .unwrap_or_else(|| crate::config::DEFAULT_STATE_REMOTE.to_string()))
}

/// Run `git push <state_remote> balls/tasks` from `dir` and classify
/// the outcome. Spawn failures propagate as Err — they're
/// catastrophic (no git on PATH) rather than a remote-state condition.
pub fn push_state_classified(dir: &Path, remote: &str) -> Result<AttemptClass> {
    let out = git::clean_git_command(dir)
        .args(["push", remote, STATE_BRANCH])
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
