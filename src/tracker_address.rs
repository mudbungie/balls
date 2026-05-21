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

use crate::config::Config;
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
