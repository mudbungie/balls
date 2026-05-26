//! Effective claim-policy resolution. Layers, lowest precedence first:
//!
//! 1. Repo-default config (`.balls/config.json`, committed to main and
//!    shared across clones).
//! 2. Per-clone override (`.balls/local/config.json`, gitignored). All
//!    fields are optional; only those set override the repo default.
//! 3. Per-invocation flag (`--sync` / `--no-sync` on `bl claim`).
//!
//! Out of scope: enforcement. A dev who flips `--no-sync` against a
//! repo whose maintainer set `require_remote_on_claim = true` is on
//! their honour. The policy guides default behaviour; the rest is
//! social.

use crate::error::Result;
use crate::participant_config::LocalPluginEntry;
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// Per-clone override of the repo-default `Config`. Stored at
/// `.balls/local/config.json`. All fields optional — `None` (or an
/// empty map) means "inherit the repo-default".
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    #[serde(default)]
    pub require_remote_on_claim: Option<bool>,
    #[serde(default)]
    pub require_remote_on_review: Option<bool>,
    #[serde(default)]
    pub require_remote_on_close: Option<bool>,
    /// SPEC §11 — per-plugin participant policy overrides. Only
    /// plugins the clone actually wants to override appear here.
    #[serde(default)]
    pub plugins: BTreeMap<String, LocalPluginEntry>,
}

impl LocalConfig {
    pub fn path(store: &Store) -> PathBuf {
        store.local_dir().join("config.json")
    }

    /// Load if present. A missing file is not an error — that's the
    /// common case. A malformed file is, so the caller knows to fix
    /// it instead of silently inheriting the repo default.
    pub fn load(store: &Store) -> Result<Option<Self>> {
        let p = Self::path(store);
        if !p.exists() {
            return Ok(None);
        }
        let s = fs::read_to_string(&p)?;
        let cfg: LocalConfig = serde_json::from_str(&s)?;
        Ok(Some(cfg))
    }
}

/// CLI-side override: which way (if any) the user pushed the toggle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SyncOverride {
    #[default]
    Unset,
    Sync,
    NoSync,
}

impl SyncOverride {
    /// Decode the `--sync` / `--no-sync` flag pair. clap declares the
    /// two flags `conflicts_with` each other, so both-set never
    /// reaches here — it folds harmlessly into `Unset` alongside the
    /// neither-set default.
    pub fn from_flags(sync: bool, no_sync: bool) -> Self {
        match (sync, no_sync) {
            (true, false) => SyncOverride::Sync,
            (false, true) => SyncOverride::NoSync,
            _ => SyncOverride::Unset,
        }
    }
}

/// Resolved claim-time policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimPolicy {
    pub require_remote: bool,
    /// True when the value comes from the repo-default config and the
    /// local clone has not previously been told about it. Drives the
    /// one-time "this repo requests synced claims" hint surfaced by
    /// `bl prime`.
    pub from_repo_default: bool,
}

/// Compute the effective claim policy for this invocation.
///
/// `local` is `LocalConfig::load(store)?` factored out so callers that
/// have already loaded it (e.g. `bl prime`'s UX path) don't read twice.
pub fn resolve(
    repo_default: bool,
    local: Option<&LocalConfig>,
    cli: SyncOverride,
) -> ClaimPolicy {
    resolve_inner(
        repo_default,
        local.and_then(|l| l.require_remote_on_claim),
        cli,
    )
}

/// Compute the effective review-time sync policy. Mirrors `resolve`
/// but reads the review-specific fields. The struct shape is shared
/// so callers can treat all three lifecycle policies uniformly.
pub fn resolve_review(
    repo_default: bool,
    local: Option<&LocalConfig>,
    cli: SyncOverride,
) -> ClaimPolicy {
    resolve_inner(
        repo_default,
        local.and_then(|l| l.require_remote_on_review),
        cli,
    )
}

/// Compute the effective close-time sync policy. See `resolve_review`.
pub fn resolve_close(
    repo_default: bool,
    local: Option<&LocalConfig>,
    cli: SyncOverride,
) -> ClaimPolicy {
    resolve_inner(
        repo_default,
        local.and_then(|l| l.require_remote_on_close),
        cli,
    )
}

fn resolve_inner(
    repo_default: bool,
    local_value: Option<bool>,
    cli: SyncOverride,
) -> ClaimPolicy {
    let after_local = local_value.unwrap_or(repo_default);
    let from_repo_default = local_value.is_none() && repo_default;
    let require_remote = match cli {
        SyncOverride::Sync => true,
        SyncOverride::NoSync => false,
        SyncOverride::Unset => after_local,
    };
    ClaimPolicy { require_remote, from_repo_default }
}

/// Reactive sync-notice: written to stderr right before `bl claim`
/// (or `claim_no_worktree`) rounds-trips through `origin/balls/tasks`
/// because of the repo-default policy. Answers "why did claim just
/// talk to origin?" at the moment of the action — no marker state.
///
/// Skipped when the user opted in explicitly (CLI `--sync` or
/// `clone.json: require_remote_on_claim: true`), since they already
/// know why the sync is happening.
pub fn emit_repo_default_sync_notice(policy: ClaimPolicy) {
    if !policy.require_remote || !policy.from_repo_default {
        return;
    }
    eprintln!(
        "syncing claim through origin/balls/tasks (repo default; \
         override with --no-sync or `clone.json: require_remote_on_claim: false`)"
    );
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
