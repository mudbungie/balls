//! `repo.json` — per-code-repo config per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §6.3.
//!
//! Lives on the code repo's own `balls/tasks` branch under
//! `.balls/repo.json`. Carries everything that varies *per code
//! repo*: how `bl review` integrates, what command gates review,
//! what protections this code repo has on `main`, where worktrees
//! go, and which remote round-trip policies are in force.
//!
//! Two read-time trip-wires:
//!
//! 1. **Tracker-scope fields are rejected.** A `repo.json` carrying
//!    `version`, `id_length`, `min_bl_version`, or `plugins` aborts
//!    read with a "tracker-scope field in repo.json" diagnostic
//!    (§6.5 / §14.9). Their primary owner is `project.json`;
//!    silently dropping would lose the field on round-trip, so
//!    failing loudly is the only correct policy.
//!
//! 2. **The pre-revision `target_branch` field is rejected.**
//!    SPEC §6.7 / §14.18: the resolution chain became
//!    `task.target_branch ?? HEAD@root`, with no repo-level
//!    default. A `repo.json` shipping `target_branch` aborts on
//!    read.
//!
//! The §6.9 lenient-unknown invariant still applies to *other*
//! unknown fields — a future forward-compat addition is observed
//! and preserved on round-trip. The two trip-wires are explicit
//! field rejections at the struct boundary, applied after parse so
//! we can name the specific field in the diagnostic.

use crate::error::{BallError, Result};
use crate::layered_fields::{Integrate, ReviewBlock};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Built-in defaults per SPEC §6.3 schema block. The effective-
/// config merger (next slice) falls through to these when neither
/// `clone.json` nor `repo.json` nor `project.json` sets the field.
pub const DEFAULT_STALE_THRESHOLD: u64 = 86_400;
pub const DEFAULT_AUTO_FETCH_ON_READY: bool = true;
pub const DEFAULT_PROTECTED_MAIN: bool = true;
pub const DEFAULT_REQUIRE_REMOTE: bool = true;

fn default_auto_fetch() -> bool {
    DEFAULT_AUTO_FETCH_ON_READY
}
fn default_stale() -> u64 {
    DEFAULT_STALE_THRESHOLD
}
fn default_protected_main() -> bool {
    DEFAULT_PROTECTED_MAIN
}

/// Per-code-repo config. All fields optional; an absent field reads
/// as its built-in default. A zero-keys `repo.json` produces a
/// fully-defaulted struct (SPEC §6.3 "ships a zero-keys `repo.json`
/// and gets the defaults").
///
/// Layered fields are `Option<>` so the effective-config merger can
/// distinguish "explicitly set" from "absent — fall through." Repo-
/// only fields read into concrete types directly via serde defaults.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoJson {
    // --- Layered fields (primary owner; project.json may default,
    //     clone.json may override). ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrate: Option<Integrate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_remote_on_claim: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_remote_on_review: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_remote_on_close: Option<bool>,

    // --- Repo-only (no project-wide overlap, no clone override
    //     for the §6.5 layered semantics — though they read through
    //     the same effective-config struct for one access seam). ---
    #[serde(default = "default_auto_fetch")]
    pub auto_fetch_on_ready: bool,
    #[serde(default = "default_stale")]
    pub stale_threshold_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_dir: Option<String>,
    #[serde(default = "default_protected_main")]
    pub protected_main: bool,
}

impl Default for RepoJson {
    /// The all-defaults shape per SPEC §6.3 — the "zero-keys
    /// repo.json reads as defaults" promise also holds when the
    /// struct is built by hand. Layered fields default to `None`
    /// (fall through to project/built-in); repo-only fields take
    /// the documented built-in default.
    fn default() -> Self {
        Self {
            integrate: None,
            review: None,
            require_remote_on_claim: None,
            require_remote_on_review: None,
            require_remote_on_close: None,
            auto_fetch_on_ready: DEFAULT_AUTO_FETCH_ON_READY,
            stale_threshold_seconds: DEFAULT_STALE_THRESHOLD,
            worktree_dir: None,
            protected_main: DEFAULT_PROTECTED_MAIN,
        }
    }
}

/// Tracker-scope fields. Each name listed here aborts the
/// `repo.json` read with §6.5's documented diagnostic when present.
/// `clone.json` reuses the same list (same rule).
pub const TRACKER_SCOPE_FIELDS: &[&str] =
    &["version", "id_length", "min_bl_version", "plugins"];

/// Reject pre-revision fields the new schema explicitly removed
/// (SPEC §6.7). `target_branch` is the only entry today.
pub const REMOVED_FIELDS: &[&str] = &["target_branch"];

impl RepoJson {
    /// Parse a `repo.json` from JSON text. Enforces both trip-wires
    /// (§6.5 / §6.7) before returning the deserialized struct.
    pub fn from_json(s: &str) -> Result<Self> {
        let raw: Value = serde_json::from_str(s)?;
        check_forbidden_fields(&raw, "repo.json")?;
        let parsed: Self = serde_json::from_value(raw)?;
        Ok(parsed)
    }

    /// Read `repo.json` from disk. `NotFound` is *not* an error: a
    /// repo with no `repo.json` reads as the all-defaults shape
    /// (SPEC §6.3). The caller decides whether absent-means-default
    /// is the right thing here — `Store::discover` always wants
    /// defaults, `bl migrate` checks for absence to decide whether
    /// to write.
    pub fn read_or_default(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => Self::from_json(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(BallError::Io(e)),
        }
    }

    /// Serialize back to canonical pretty JSON text.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(BallError::Json)
    }
}

/// Shared between `repo.json` and `clone.json`: scan a parsed JSON
/// object for tracker-scope or removed fields, aborting with the
/// documented diagnostic when either is present. Public so
/// `clone_json` can call it with its own file label.
pub fn check_forbidden_fields(raw: &Value, file_label: &str) -> Result<()> {
    let Value::Object(map) = raw else {
        return Ok(());
    };
    for forbidden in TRACKER_SCOPE_FIELDS {
        if map.contains_key(*forbidden) {
            return Err(BallError::InvalidTask(format!(
                "tracker-scope field in {file_label}: {forbidden:?} \
                 (owned by project.json — see SPEC-clone-layout §6.5)"
            )));
        }
    }
    for removed in REMOVED_FIELDS {
        if map.contains_key(*removed) {
            return Err(BallError::InvalidTask(format!(
                "removed field in {file_label}: {removed:?} \
                 (see SPEC-clone-layout §6.7)"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "repo_json_tests.rs"]
mod tests;
