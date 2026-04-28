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
use std::io::Write;
use std::path::PathBuf;

/// Per-clone override of the repo-default `Config`. Stored at
/// `.balls/local/config.json`. All fields optional — `None` (or an
/// empty map) means "inherit the repo-default".
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    #[serde(default)]
    pub require_remote_on_claim: Option<bool>,
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
    let local_value = local.and_then(|l| l.require_remote_on_claim);
    let after_local = local_value.unwrap_or(repo_default);
    let from_repo_default = local_value.is_none() && repo_default;
    let require_remote = match cli {
        SyncOverride::Sync => true,
        SyncOverride::NoSync => false,
        SyncOverride::Unset => after_local,
    };
    ClaimPolicy { require_remote, from_repo_default }
}

/// Marker file path (under `.balls/local/`) recording that this clone
/// has already seen — and been notified about — the repo-default
/// claim-sync policy. The file's mere existence is the signal; its
/// contents are informative only.
fn seen_marker_path(store: &Store) -> PathBuf {
    store.local_dir().join("seen-claim-sync-policy")
}

/// One-time hint, written to stderr the first time a clone sees the
/// repo-default `require_remote_on_claim` set to true. Mitigates the
/// "surprise: my claims are hitting the network" risk for new devs
/// onboarding to a project. Subsequent invocations are silent.
///
/// Writing the marker is best-effort: if `.balls/local/` isn't
/// writable, we'd rather repeat the hint than fail the prime.
pub fn notify_repo_default_once(store: &Store, policy: ClaimPolicy) {
    if !policy.from_repo_default || !policy.require_remote {
        return;
    }
    let marker = seen_marker_path(store);
    if marker.exists() {
        return;
    }
    let _ = writeln!(
        std::io::stderr(),
        "this repo requests synced claims (remote default; override with --no-sync \
         or `.balls/local/config.json` `require_remote_on_claim: false`)"
    );
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, "shown\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_sync_overrides_everything() {
        let p = resolve(false, None, SyncOverride::Sync);
        assert!(p.require_remote);
    }

    #[test]
    fn cli_no_sync_overrides_repo_and_local() {
        let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: Some(true), ..Default::default() }), SyncOverride::NoSync);
        assert!(!p.require_remote);
    }

    #[test]
    fn local_override_beats_repo_default() {
        let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: Some(false), ..Default::default() }), SyncOverride::Unset);
        assert!(!p.require_remote);
        assert!(!p.from_repo_default);
    }

    #[test]
    fn unset_local_falls_through_to_repo_default() {
        let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: None, ..Default::default() }), SyncOverride::Unset);
        assert!(p.require_remote);
        assert!(p.from_repo_default);
    }

    #[test]
    fn no_local_file_falls_through_to_repo_default() {
        let p = resolve(true, None, SyncOverride::Unset);
        assert!(p.require_remote);
        assert!(p.from_repo_default);
    }

    #[test]
    fn off_by_default_when_nothing_set() {
        let p = resolve(false, None, SyncOverride::Unset);
        assert!(!p.require_remote);
        assert!(!p.from_repo_default);
    }

    #[test]
    fn from_repo_default_false_when_local_explicitly_matches() {
        // Local explicitly says true; not "inherited from repo".
        let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: Some(true), ..Default::default() }), SyncOverride::Unset);
        assert!(p.require_remote);
        assert!(!p.from_repo_default);
    }
}
