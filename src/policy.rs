//! Effective claim-policy resolution. Layers, lowest precedence first:
//!
//! 1. Repo-default config (`repo.json` on the tracker branch under
//!    XDG, `.balls/config.json` under legacy).
//! 2. Per-clone override — `clone.json` (SPEC §6.4) — XDG-only. The
//!    legacy `.balls/local/config.json` reader retired with bl-5a03:
//!    legacy clones lose their per-clone override surface until they
//!    migrate (`bl doctor` flags lingering local/config.json files).
//! 3. Per-invocation flag (`--sync` / `--no-sync`).
//!
//! Out of scope: enforcement. A dev who flips `--no-sync` against a
//! repo whose maintainer set `require_remote_on_claim = true` is on
//! their honour. The policy guides default behaviour; the rest is
//! social.

use crate::clone_json::CloneJson;
use crate::store::Store;

/// In-memory view of this clone's `require_remote_on_*` overrides —
/// the slice of `clone.json` the policy resolvers consume. Holding it
/// as its own type lets `policy::resolve*` accept any source (clone.json
/// today; whatever the next SPEC revision moves it to) without
/// re-shaping the call sites.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalConfig {
    pub require_remote_on_claim: Option<bool>,
    pub require_remote_on_review: Option<bool>,
    pub require_remote_on_close: Option<bool>,
}

impl LocalConfig {
    /// Project the policy-relevant fields out of a `clone.json`. Used
    /// by `commands::plumbing::sync_inputs` once the store has loaded
    /// its clone.json at discovery.
    #[must_use]
    pub fn from_clone(cj: &CloneJson) -> Self {
        Self {
            require_remote_on_claim: cj.require_remote_on_claim,
            require_remote_on_review: cj.require_remote_on_review,
            require_remote_on_close: cj.require_remote_on_close,
        }
    }

    /// Load this clone's overrides from the store. `None` ⇒ no
    /// override layer (no clone.json present, or legacy layout).
    #[must_use]
    pub fn load(store: &Store) -> Option<Self> {
        store.clone_json().map(Self::from_clone)
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
/// `local` is `LocalConfig::load(store)` factored out so callers that
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
