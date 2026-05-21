use crate::error::{BallError, Result};
use crate::git;
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

/// Which `bl review` code path a repo follows (SPEC §5).
///
/// `LocalSquash` is today's behavior — squash the work branch into the
/// integration branch locally and immediately. `Deferred` hands the
/// squash off to an external forge: `bl review` pushes the work branch
/// and opens an auto-gate instead of touching the integration branch.
/// Absent config ⇒ `LocalSquash`, byte-identical to before this field.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryMode {
    #[default]
    LocalSquash,
    Deferred,
}

/// The `delivery` config block. Its own struct (rather than a bare
/// `DeliveryMode` field) so future delivery knobs extend it without
/// another top-level key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    #[serde(default)]
    pub mode: DeliveryMode,
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
    /// Git remote **name** whose `balls/tasks` ref this repo negotiates
    /// against. `None` (the default) resolves to `origin`. Set to point
    /// at a shared task hub via an existing project-side remote.
    ///
    /// **Deprecated by `master_url` (target: 0.5.0).** The name field
    /// lives in per-clone `.git/config` and so doesn't travel via a
    /// fresh `git clone`; `master_url` carries the URL in committed
    /// config and provisions a balls-owned checkout automatically.
    /// When both are set, `master_url` wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_remote: Option<String>,
    /// Hub URL of an external task master (bl-ffb4). When set, balls
    /// materializes its own git clone at `.balls/state-repo/` and
    /// routes every state-branch op through it; the project's own
    /// `.git/config` stays clean. Wins over legacy `state_remote`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_url: Option<String>,
    /// The integration branch `bl review` squashes into, `bl sync`
    /// pushes alongside the state branch, and the delivery-link /
    /// half-push tag scans consult. `None` (the default, and every
    /// config written before this field) falls back to whatever
    /// branch is checked out at the repo root — today's *implicit*
    /// target, which a stray `git checkout` at the root silently
    /// re-points. Set this to pin the target explicitly: `develop`
    /// for a git-flow repo, `main` for a hotfix worktree. Resolved
    /// through the single `integration_branch()` seam below, mirroring
    /// `state_remote` — unset is byte-identical to before this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
    /// Opt-in delivery mode (SPEC §5). `None` (the default, and every
    /// config written before this field) means `local-squash` — the
    /// squash lands locally as it always has. `Some` with
    /// `mode = "deferred"` makes `bl review` push the work branch and
    /// open a forge gate instead. Skipped from serialization when
    /// unset so an untouched config stays byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Delivery>,
    /// Advisory minimum `bl` version (SPEC §5 / §10). `None` (the
    /// default, and every config written before this field) is silent.
    /// When set, a client below it warns on load; older clients drop
    /// the unknown field entirely. Advisory only — no engineering
    /// prevention; see `min_version`. Skipped when unset so an
    /// untouched config stays byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_bl_version: Option<String>,
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginEntry>,
}

/// The remote a config with no explicit `state_remote` targets.
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
            master_url: None,
            target_branch: None,
            delivery: None,
            min_bl_version: None,
            plugins: BTreeMap::new(),
        }
    }
}

pub const ID_LENGTH_MIN: usize = 4;
pub const ID_LENGTH_MAX: usize = 32;
/// On-disk schema version for `.balls/config.json`. Older clients
/// reject a higher number with a clear "bl is too old" error. New
/// fields carry serde defaults so older configs load unchanged.
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
        crate::min_version::warn_if_below(c.min_bl_version.as_deref());
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
    /// this binary understands. `pub` so bl-32e5's admin surface re-runs the gate before persisting.
    pub fn validate(&self) -> Result<()> {
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
        validate_drop_policies(&self.plugins)
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

    /// Resolved hub URL for the balls-owned state checkout (bl-ffb4),
    /// or `None` for the legacy `state_remote`-name model. The seam
    /// that branches between the two layouts.
    pub fn master_url(&self) -> Option<&str> {
        self.master_url.as_deref()
    }

    /// Resolve the integration branch — the single seam every consumer
    /// (`bl review`'s squash, `bl sync`'s push, delivery-link and
    /// half-push tag scans, `bl claim`'s worktree base) routes through.
    /// Configured value wins; otherwise falls back to `HEAD@root`,
    /// byte-identical to the implicit pre-field behavior.
    pub fn integration_branch(&self, root: &Path) -> Result<String> {
        match &self.target_branch {
            Some(b) => Ok(b.clone()),
            None => git::git_current_branch(root),
        }
    }

    /// Resolve the effective integration branch for delivering one
    /// specific task. A task's own `target_branch` is the per-task
    /// override — the smallest unit that expresses git-flow's
    /// hotfix→main vs feature→develop split without a parallel
    /// "branch type" lifecycle. It wins outright; otherwise this
    /// falls back to the repo-level `integration_branch()` seam. This
    /// is the per-task analogue of that seam, so the full precedence
    ///   `task.target_branch ?? config.target_branch ?? HEAD@root`
    /// lives in exactly one place. Every consumer that delivers or
    /// resolves a *single task* (review's squash, claim's worktree
    /// catch-up, show's delivery scan) routes through here.
    pub fn integration_branch_for(
        &self,
        root: &Path,
        task_target: Option<&str>,
    ) -> Result<String> {
        match task_target {
            Some(b) => Ok(b.to_string()),
            None => self.integration_branch(root),
        }
    }

    /// Resolved delivery mode (`None` block ⇒ `LocalSquash`). Single
    /// seam `bl review` consults to pick its code path.
    pub fn delivery_mode(&self) -> DeliveryMode {
        self.delivery
            .as_ref()
            .map(|d| d.mode.clone())
            .unwrap_or_default()
    }
}

/// SPEC §6.2: `drop` is observe-only — only `best-effort` is legal.
/// Lifted out of `validate()` to keep `config.rs` under the 300-line cap.
fn validate_drop_policies(plugins: &BTreeMap<String, PluginEntry>) -> Result<()> {
    for (name, entry) in plugins {
        let Some(ep) = entry
            .participant
            .as_ref()
            .and_then(|p| p.subscriptions.get(&crate::participant::Event::Drop))
        else {
            continue;
        };
        if !matches!(ep.policy, crate::participant_config::PolicyKind::BestEffort) {
            return Err(BallError::Other(format!(
                "invalid config: plugin {name:?} subscribes to `drop` with a \
                 non-best-effort policy; drop is observe-only (SPEC §6.2)"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "config_schema_tests.rs"]
mod schema_tests;
