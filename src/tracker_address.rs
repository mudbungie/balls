//! Tracker-address resolution (SPEC-tracker-state §5).
//!
//! A workspace's address is the pair `(state_url, state_branch)`,
//! both optional in `.balls/config.json`. Absent ⇒ the implicit
//! default: the code repo's own `origin`, branch `balls/tasks`. This
//! module is the one seam that turns a `Config` plus a repo root into
//! a concrete `Address` — every materialization routes through it, so
//! "standalone" and "federated" are one resolution differing only in
//! the values, never a mode branch.
//!
//! Legacy migration: a pre-spec repo may carry the address in the
//! retired `.balls/master.json` pointer, or as `master_url` /
//! `state_remote` fields inside `config.json`. `resolve` reads all
//! three transparently; `bl remaster` rewrites them to `state_url`.
//!
//! `state_branch` half: bl-8a9a wired the *resolution* (here) and the
//! *materialization* (`state_repo`) of a non-default branch, but not
//! the lifecycle traffic — so `ensure_supported` gates it (see there).

use crate::config::Config;
use crate::error::{BallError, Result};
use crate::git_state;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// The default state branch when `state_branch` is unset.
pub const DEFAULT_BRANCH: &str = "balls/tasks";

/// A resolved tracker address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// The tracker URL, or `None` for a solo repo with no `origin` —
    /// the offline-bootstrappable implicit default (§9).
    pub url: Option<String>,
    /// The state branch name.
    pub branch: String,
    /// True when the address is explicitly configured (a `state_url`
    /// or a legacy `master_url`). An explicit address hard-fails first
    /// contact when unreachable; the implicit default — and a legacy
    /// `state_remote` name, kept safe-but-unlinked — falls back to a
    /// local checkout instead (§9).
    pub explicit: bool,
}

#[derive(Debug, Default, Deserialize)]
struct LegacyPointer {
    master_url: Option<String>,
    state_remote: Option<String>,
}

/// Resolve the tracker address for the workspace at `root`, given its
/// already-loaded `config.json`. Never touches the network.
pub fn resolve(root: &Path, cfg: &Config) -> Address {
    let branch = cfg
        .state_branch
        .clone()
        .unwrap_or_else(|| DEFAULT_BRANCH.to_string());

    // Explicit state_url wins outright.
    if let Some(url) = &cfg.state_url {
        return Address { url: Some(url.clone()), branch, explicit: true };
    }

    // Legacy: the master.json pointer, then in-config master_url.
    let legacy = read_legacy(root);
    if let Some(url) = legacy.master_url.or_else(|| cfg.master_url.clone()) {
        return Address { url: Some(url), branch, explicit: true };
    }

    // Legacy: a state_remote *name* — resolve it to a URL live. It
    // stays non-explicit (safe-but-unlinked on an unreachable hub,
    // bl-8e8f); an unresolvable name degrades to the implicit default.
    if let Some(name) = legacy.state_remote.or_else(|| cfg.state_remote.clone()) {
        if let Some(url) = git_state::remote_url(root, &name) {
            return Address { url: Some(url), branch, explicit: false };
        }
    }

    // Implicit default: the code repo's own `origin`, resolved live.
    Address { url: git_state::remote_url(root, "origin"), branch, explicit: false }
}

/// Reject a tracker address this `bl` cannot honor end to end.
///
/// `state_branch` is *resolved* (`resolve`, above) and *materialized*
/// — bl-8a9a checks the configured branch out in `.balls/state-repo` —
/// but the claim/review/close/sync traffic still hardcodes
/// `git_state::STATE_BRANCH` (`DEFAULT_BRANCH`). A non-default value
/// would leave the local state branch and every push/fetch/merge
/// refspec naming different branches: a silently half-working field,
/// worse than an honest "not yet". Until that traffic is wired
/// (SPEC-tracker-state §5 / §8; follow-up bl-3f59) a non-default
/// `state_branch` is a hard error. Called from `Config::validate`, so
/// it fronts every command — the field has no CLI writer today, so the
/// only way in is a hand-edit, and the only way out is the same.
pub fn ensure_supported(cfg: &Config) -> Result<()> {
    if let Some(branch) = &cfg.state_branch {
        if branch != DEFAULT_BRANCH {
            return Err(BallError::Other(format!(
                "invalid config: state_branch {branch:?} is not yet \
                 supported by this bl — the claim/review/close/sync \
                 paths still target {DEFAULT_BRANCH:?}, so a non-default \
                 branch would silently misroute task state \
                 (SPEC-tracker-state §5 is not wired end to end; see \
                 bl-3f59). Remove `state_branch` from .balls/config.json, \
                 or set it to {DEFAULT_BRANCH:?}."
            )));
        }
    }
    Ok(())
}

/// Read the retired `.balls/master.json` pointer if present. A missing
/// or unreadable file is "no legacy pointer" — standalone.
fn read_legacy(root: &Path) -> LegacyPointer {
    let p = root.join(".balls/master.json");
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "tracker_address_tests.rs"]
mod tests;
