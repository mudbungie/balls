//! `tracker.json` — strict pointer-only redirect file per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §6.1.
//!
//! Lives on a clone's *own* `origin balls/tasks` branch under
//! `.balls/tracker.json`. The file's **presence** is the redirect
//! signal (§5): present → this repo redirects to a federated
//! tracker; absent → the repo's own branch is the tracker.
//!
//! Two fields only — `state_url` (required) and `state_branch`
//! (optional, defaults to `balls/tasks`). Any other field aborts
//! read (`deny_unknown_fields`). The justification in §6.1: the
//! file is so small that an unknown field is almost certainly a
//! stale write from an older binary, and round-tripping makes the
//! redirect mechanism look more configurable than it is.
//!
//! Writing a `tracker.json` with no `state_url` is illegal — that
//! would be a redirect to nowhere. `bl remaster` either writes a
//! real pointer or removes the file; this module enforces it on the
//! read side (`state_url` is a non-`Option` field).
//!
//! No wiring into `Store::discover` yet — that lands in a later
//! bl-77cb slice when `discover` learns to follow the redirect.

use crate::error::{BallError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Default state branch (§6.1) when `tracker.json` omits the field.
/// Matches `encoding::ENC_BALLS_TASKS` after percent-encoding.
pub const DEFAULT_STATE_BRANCH: &str = "balls/tasks";

/// Strict pointer-only schema. `deny_unknown_fields` is the §14.6
/// gate; serde's "unknown field" error surfaces through the
/// loader's error mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackerJson {
    /// Federated tracker repo URL. Required: a `tracker.json` with
    /// no `state_url` is the "redirect to nowhere" the SPEC rules
    /// out at §6.1.
    pub state_url: String,
    /// Branch on `state_url` carrying the tracker state. Optional;
    /// `None` resolves to [`DEFAULT_STATE_BRANCH`] at the call
    /// site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_branch: Option<String>,
}

impl TrackerJson {
    /// Parse a `tracker.json` from JSON text. Strict on unknown
    /// fields (§14.6); the diagnostic comes through `BallError::Json`.
    pub fn from_json(s: &str) -> Result<Self> {
        let parsed: Self = serde_json::from_str(s)?;
        if parsed.state_url.is_empty() {
            return Err(BallError::InvalidTask(
                "tracker.json: state_url is empty (a tracker.json with no \
                 state_url is a redirect to nowhere — see SPEC-clone-layout §6.1)"
                    .into(),
            ));
        }
        Ok(parsed)
    }

    /// Read `tracker.json` from disk at the given path. Returns
    /// `Ok(None)` if the file is absent — the bootstrap signal (§5)
    /// is presence-or-absence, so "missing" is not an error.
    pub fn read_optional(path: &Path) -> Result<Option<Self>> {
        match fs::read_to_string(path) {
            Ok(s) => Self::from_json(&s).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(BallError::Io(e)),
        }
    }

    /// The effective state branch — explicit value or the default.
    /// SPEC §6.1 lets callers stop checking `state_branch.is_some()`
    /// at every call site.
    #[must_use]
    pub fn effective_branch(&self) -> &str {
        self.state_branch.as_deref().unwrap_or(DEFAULT_STATE_BRANCH)
    }

    /// Serialize back to canonical JSON text. Used by `bl remaster`
    /// when writing the redirect; here it's the round-trip pair to
    /// `from_json` (used by tests).
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(BallError::Json)
    }
}

#[cfg(test)]
#[path = "tracker_json_tests.rs"]
mod tests;
