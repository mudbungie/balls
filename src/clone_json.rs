//! `clone.json` — per-on-disk-checkout override per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §6.4.
//!
//! **Never committed.** Lives at
//! `~/.config/balls/<nested-clone-path>/clone.json`. Present only
//! when something at this on-disk checkout legitimately differs
//! from the repo or project defaults — a stealth marker, a custom
//! worktree path on this developer's machine, a one-off override
//! of `require_remote_on_close`.
//!
//! Two fields are clone.json's *own* (no analogue in `repo.json`
//! or `project.json`): `stealth` and `tasks_dir`. The rest are
//! optional layered-field overrides — same types as the layered
//! fields on `repo.json`, with the precedence rule
//! `clone.json ?? repo.json ?? project.json ?? built-in default`
//! resolved by [`crate::effective_config`].
//!
//! The same §6.5 trip-wire as `repo.json` applies here: tracker-
//! scope fields (`version`, `id_length`, `min_bl_version`,
//! `plugins`) abort the read with a "tracker-scope field in
//! clone.json" diagnostic. `repo_json::check_forbidden_fields` is
//! reused with this file's label.

use crate::error::{BallError, Result};
use crate::layered_fields::{Integrate, ReviewBlock};
use crate::repo_json::check_forbidden_fields;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Per-on-disk-checkout override. All fields optional; an absent
/// field is "no override at this layer" (the merger falls through
/// to `repo.json`).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CloneJson {
    /// Stealth mode flag. SPEC §4.1: when `true`, `Store::discover`
    /// short-circuits the entire origin / `trackers/` / redirect
    /// machinery and reads tasks directly from `tasks_dir`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stealth: bool,

    /// When `stealth = true`, the absolute path to the task store.
    /// Required in stealth mode (validated on read); ignored
    /// otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks_dir: Option<String>,

    // --- Layered overrides — same types as repo.json. clone.json
    //     winning over repo.json is the §6.5 precedence pattern. ---
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_fetch_on_ready: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_threshold_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_main: Option<bool>,
}

// serde's `skip_serializing_if` calls a `fn(&T) -> bool`, so the
// reference is forced by the API.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(b: &bool) -> bool {
    !*b
}

impl CloneJson {
    /// Parse `clone.json` from JSON text. Enforces the §6.5
    /// tracker-scope trip-wire and the §6.7 removed-fields list,
    /// then validates the stealth-requires-tasks-dir invariant.
    pub fn from_json(s: &str) -> Result<Self> {
        let raw: Value = serde_json::from_str(s)?;
        check_forbidden_fields(&raw, "clone.json")?;
        let parsed: Self = serde_json::from_value(raw)?;
        parsed.validate_stealth()?;
        Ok(parsed)
    }

    /// Read `clone.json` from disk. `NotFound` → `Ok(None)` —
    /// absence means "no per-clone overrides," not an error
    /// (clone.json is present *only when set* per §6.4).
    pub fn read_optional(path: &Path) -> Result<Option<Self>> {
        match fs::read_to_string(path) {
            Ok(s) => Self::from_json(&s).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(BallError::Io(e)),
        }
    }

    /// Write to disk, creating parent directories as needed. The
    /// caller picks the path via `xdg_paths::clone_json_path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, s + "\n")?;
        Ok(())
    }

    /// SPEC §4.1: `stealth: true` requires `tasks_dir`. The pair
    /// describes the on-disk checkout's relationship to git itself
    /// — bl bypasses every git lookup in stealth mode, so the
    /// task store path must be explicit.
    fn validate_stealth(&self) -> Result<()> {
        if self.stealth && self.tasks_dir.is_none() {
            return Err(BallError::InvalidTask(
                "clone.json: stealth=true requires tasks_dir \
                 (see SPEC-clone-layout §4.1)"
                    .into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "clone_json_tests.rs"]
mod tests;
