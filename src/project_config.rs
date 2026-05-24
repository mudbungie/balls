//! `.balls/project.json` — the project-owned half of balls config
//! (SPEC-tracker-state §7).
//!
//! Config has two owners. The *repo* owns how this code repo builds
//! and integrates (`Config`, `.balls/config.json`, committed to the
//! code branch). The *project* owns the shared backlog policy — the
//! store schema version, the task-id width, the advisory `bl` floor,
//! and the plugin map — and that lives here, on the tracker branch,
//! inherited by every clone through the `.balls/project.json` symlink.
//!
//! On overlap `project.json` wins outright: a stale `config.json` copy
//! of a project-owned field is shadowed, never honored. A repo
//! predating the split has no `project.json`; its project-owned fields
//! are read from `config.json` until `state_repo::seed` migrates them.

use crate::error::{BallError, Result};
use crate::participant::Event;
use crate::participant_config::{ParticipantConfig, PolicyKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// One plugin's entry in the project-owned `plugins` map. Lives here
/// (not in `config`) because the map is project config; `config`
/// re-exports it so `config::PluginEntry` stays valid for callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sync_on_change: bool,
    pub config_file: String,
    /// SPEC §11 — optional per-event participant policy. Absent on
    /// legacy configs; the resolver falls through to the
    /// `sync_on_change` mapping when this is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant: Option<ParticipantConfig>,
}

pub const ID_LENGTH_MIN: usize = 4;
pub const ID_LENGTH_MAX: usize = 32;

/// On-disk schema version for `.balls/project.json`. Older clients
/// reject a higher number with a clear "bl is too old" error; new
/// fields carry serde defaults so older files load unchanged.
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

fn default_version() -> u32 {
    PROJECT_SCHEMA_VERSION
}

fn default_id_length() -> usize {
    4
}

/// Project-owned configuration: the fields every clone sharing a
/// tracker inherits. Read through the `.balls/project.json` symlink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Store schema version (SPEC §7). Defaulted so a `config.json`
    /// predating the split — which carries no `version` once the field
    /// migrates out — still resolves.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Hex width of a minted task id. Must be consistent for every id
    /// in the shared store, which is why it is project-owned.
    #[serde(default = "default_id_length")]
    pub id_length: usize,
    /// Advisory minimum `bl` version (SPEC §5 / §10). `None` is silent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_bl_version: Option<String>,
    /// The plugin map: which external-tracker plugins the project runs.
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginEntry>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            version: PROJECT_SCHEMA_VERSION,
            id_length: default_id_length(),
            min_bl_version: None,
            plugins: BTreeMap::new(),
        }
    }
}

impl ProjectConfig {
    /// Load and validate a `project.json`. The same struct also reads a
    /// `config.json`'s project-owned fields — serde drops the
    /// repo-owned keys — which is how the pre-split fallback and
    /// the `state_repo::seed` migration share one parser.
    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BallError::config_missing(path),
            _ => BallError::Io(e),
        })?;
        let mut c: ProjectConfig = serde_json::from_str(&s)?;
        c.sanitize();
        c.validate()?;
        crate::min_version::warn_if_below(c.min_bl_version.as_deref());
        Ok(c)
    }

    /// The project-owned view of a `config.json`, for a repo predating
    /// the config split. Lenient: a missing or unparseable file yields
    /// built-in defaults — the file's validity is the repo `Config`
    /// loader's concern, surfaced there.
    pub fn from_config_file(config_path: &Path) -> Self {
        let Ok(s) = fs::read_to_string(config_path) else {
            return Self::default();
        };
        let Ok(mut c) = serde_json::from_str::<ProjectConfig>(&s) else {
            return Self::default();
        };
        c.sanitize();
        crate::min_version::warn_if_below(c.min_bl_version.as_deref());
        c
    }

    /// Resolve the effective project config. `project.json` wins
    /// outright when it exists; a repo without one — stealth, or a
    /// pre-split checkout — falls back to `config.json`'s project-owned
    /// fields, still validated.
    pub fn resolve(project_json: &Path, config_json: &Path) -> Result<Self> {
        if project_json.exists() {
            return Self::load(project_json);
        }
        let c = Self::from_config_file(config_json);
        c.validate()?;
        Ok(c)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, s + "\n")?;
        Ok(())
    }

    /// Clamp `id_length` into the supported range, warning on clamp.
    /// `id_length = 0` would otherwise infinite-loop id generation;
    /// very large values waste hex space without colliding any less.
    fn sanitize(&mut self) {
        if !(ID_LENGTH_MIN..=ID_LENGTH_MAX).contains(&self.id_length) {
            let original = self.id_length;
            self.id_length = self.id_length.clamp(ID_LENGTH_MIN, ID_LENGTH_MAX);
            eprintln!(
                "warning: id_length {} out of range [{}, {}]; clamped to {}",
                original, ID_LENGTH_MIN, ID_LENGTH_MAX, self.id_length
            );
        }
    }

    /// Reject a schema version newer than this binary understands, and
    /// any plugin subscribing to `drop` with a non-best-effort policy.
    /// `pub` so the plugin admin surface re-runs the gate before
    /// persisting.
    pub fn validate(&self) -> Result<()> {
        if self.version > PROJECT_SCHEMA_VERSION {
            return Err(BallError::Other(format!(
                "project schema version {} is newer than this bl (supports up to {}); \
                 upgrade bl to read this repo's project config",
                self.version, PROJECT_SCHEMA_VERSION
            )));
        }
        validate_drop_policies(&self.plugins)
    }
}

/// SPEC §6.2: `drop` is observe-only — only `best-effort` is legal.
fn validate_drop_policies(plugins: &BTreeMap<String, PluginEntry>) -> Result<()> {
    for (name, entry) in plugins {
        let Some(ep) = entry
            .participant
            .as_ref()
            .and_then(|p| p.subscriptions.get(&Event::Drop))
        else {
            continue;
        };
        if !matches!(ep.policy, PolicyKind::BestEffort) {
            return Err(BallError::Other(format!(
                "invalid config: plugin {name:?} subscribes to `drop` with a \
                 non-best-effort policy; drop is observe-only (SPEC §6.2)"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "project_config_tests.rs"]
mod tests;
