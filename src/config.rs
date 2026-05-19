use crate::error::{BallError, Result};
use crate::participant_config::ParticipantConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub id_length: usize,
    pub stale_threshold_seconds: u64,
    #[serde(default = "default_true")]
    pub auto_fetch_on_ready: bool,
    pub worktree_dir: String,
    #[serde(default)]
    pub protected_main: bool,
    /// When true, `bl claim` round-trips the claim commit through
    /// `origin/balls/tasks` before creating the worktree. Closes the
    /// offline-agent claim race; off by default. Per-clone override
    /// via `.balls/local/config.json`; per-invocation override via
    /// `bl claim --sync` / `--no-sync`. Same precedence chain for
    /// the review/close variants below.
    #[serde(default)]
    pub require_remote_on_claim: bool,
    #[serde(default)]
    pub require_remote_on_review: bool,
    #[serde(default)]
    pub require_remote_on_close: bool,
    /// Git remote whose `balls/tasks` ref is authoritative for this
    /// repo. `None` (the default, and every config written before
    /// this field existed) resolves to `origin` — see
    /// `state_remote()`. Set to a different remote to point a client
    /// repo at a shared task hub: every `balls/tasks` fetch/push then
    /// negotiates against the hub through the same git-remote
    /// participant, while the code remote (`origin`) is untouched.
    /// Lives in the committed config so the project link travels with
    /// the codebase; a fork detaches via the per-clone override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_remote: Option<String>,
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginEntry>,
}

/// The remote a config with no explicit `state_remote` targets. Equal
/// to today's hardcoded behavior, so an unmodified config produces
/// byte-identical git invocations.
pub const DEFAULT_STATE_REMOTE: &str = "origin";

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CONFIG_SCHEMA_VERSION,
            id_length: 4,
            stale_threshold_seconds: 60,
            auto_fetch_on_ready: true,
            worktree_dir: ".balls-worktrees".to_string(),
            protected_main: false,
            require_remote_on_claim: false,
            require_remote_on_review: false,
            require_remote_on_close: false,
            state_remote: None,
            plugins: BTreeMap::new(),
        }
    }
}

pub const ID_LENGTH_MIN: usize = 4;
pub const ID_LENGTH_MAX: usize = 32;

/// Current on-disk schema version for `.balls/config.json`. Bump this
/// when a config change requires migration logic. Older clients
/// reading a config written with a higher version refuse to load
/// with a clear "your bl is too old" error rather than silently
/// losing fields. Lower-or-equal versions load normally because the
/// struct definition is backward-compatible by design (new fields
/// carry serde defaults).
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BallError::config_missing(path),
            _ => BallError::Io(e),
        })?;
        let mut c: Config = serde_json::from_str(&s)?;
        c.sanitize();
        c.validate()?;
        Ok(c)
    }

    /// Clamp `id_length` into the supported range, warning on clamp.
    /// id_length = 0 would otherwise infinite-loop id generation; very large
    /// values waste hex space without colliding any less.
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

    /// Reject `worktree_dir` values that would escape the repo root,
    /// and refuse configs written with a schema version newer than
    /// this binary understands.
    fn validate(&self) -> Result<()> {
        if self.version > CONFIG_SCHEMA_VERSION {
            return Err(BallError::Other(format!(
                "config schema version {} is newer than this bl (supports up to {}); \
                 upgrade bl to read this repo's config",
                self.version, CONFIG_SCHEMA_VERSION
            )));
        }
        if self.worktree_dir.starts_with('/') || self.worktree_dir.contains("..") {
            return Err(BallError::Other(format!(
                "invalid config: worktree_dir {:?} must be a relative path with no '..' segments",
                self.worktree_dir
            )));
        }
        // SPEC §6.2: `drop` is observe-only. A `required` or `gating`
        // policy on it is rejected here — an observer must never be
        // able to block or stage a local claim release (§2 soft
        // policy, hard primitive). Only `best-effort` is legal.
        for (name, entry) in &self.plugins {
            let drop_policy = entry
                .participant
                .as_ref()
                .and_then(|p| p.subscriptions.get(&crate::participant::Event::Drop));
            if let Some(ep) = drop_policy {
                if !matches!(ep.policy, crate::participant_config::PolicyKind::BestEffort) {
                    return Err(BallError::Other(format!(
                        "invalid config: plugin {name:?} subscribes to `drop` with a \
                         non-best-effort policy; drop is observe-only (SPEC §6.2)"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, s + "\n")?;
        Ok(())
    }

    /// The git remote whose `balls/tasks` ref this repo negotiates
    /// against. This is the single seam that knows the `state_remote`
    /// default: every state-branch fetch/push (init, the git-remote
    /// participant, `bl sync`) resolves the remote through here, so
    /// there is no second code path with the `origin` fallback baked
    /// in. `None` ⇒ `origin`, byte-identical to a single-repo setup.
    pub fn state_remote(&self) -> &str {
        self.state_remote.as_deref().unwrap_or(DEFAULT_STATE_REMOTE)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
