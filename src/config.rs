use crate::error::{BallError, Result};
use crate::git;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

// The nested config blocks live in `config_blocks.rs` (line-cap split,
// bl-1f38); re-exported here so `config::Delivery` etc. are unchanged.
pub use crate::config_blocks::{Delivery, DeliveryMode, ReviewConfig};
// `PluginEntry` is project config (SPEC §7) and lives in
// `project_config`; re-exported so `config::PluginEntry` still resolves
// for the callers that predate the split.
pub use crate::project_config::PluginEntry;

/// Repo-owned configuration (SPEC-tracker-state §7): how *this* code
/// repo builds, integrates, and where its task state lives. A real,
/// never-symlinked `.balls/config.json`, committed to the code branch.
/// The project-owned half — schema version, id width, plugin map — is
/// `ProjectConfig` (`.balls/project.json`, on the tracker branch).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
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
    /// The tracker address (SPEC-tracker-state §5): where the shared
    /// `balls/tasks` branch lives. `None` resolves live to the code
    /// repo's own `origin` — a standalone repo carries neither field,
    /// which is why a pre-federation config is already conformant.
    /// `bl remaster <url>` writes `state_url`; `bl remaster --detach`
    /// removes it. Resolved through `tracker_address::resolve`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_url: Option<String>,
    /// The tracker's branch name. `None` resolves to `balls/tasks`.
    /// Lets one tracker host several projects on distinct branches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_branch: Option<String>,
    /// Legacy bootstrap fields, read transparently for migration.
    /// A pre-spec `.balls/config.json` (or `.balls/master.json`) may
    /// still carry `master_url` / `state_remote`; `tracker_address`
    /// folds them into the `state_url` resolution and `bl remaster`
    /// rewrites them away. New code never writes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_remote: Option<String>,
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
    /// Opt-in pre-squash review gate (bl-1f38). When `review.pre_check`
    /// is set, `bl review` runs that command in the worktree after
    /// merging the integration branch in and aborts the review if it
    /// exits non-zero — so the quality gate runs at the merge, not just
    /// in CI. `None` (the default, and every config written before this
    /// field) ⇒ no gate. Skipped from serialization when unset so an
    /// untouched config stays byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewConfig>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            stale_threshold_seconds: 60,
            auto_fetch_on_ready: true,
            worktree_dir: ".balls-worktrees".to_string(),
            protected_main: false,
            require_remote_on_claim: false,
            require_remote_on_review: false,
            require_remote_on_close: false,
            state_url: None,
            state_branch: None,
            state_remote: None,
            master_url: None,
            target_branch: None,
            delivery: None,
            review: None,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BallError::config_missing(path),
            _ => BallError::Io(e),
        })?;
        let c: Config = serde_json::from_str(&s)?;
        c.validate()?;
        Ok(c)
    }

    /// Reject `worktree_dir` values that would escape the repo root.
    /// `pub` so bl-32e5's admin surface re-runs the gate before
    /// persisting. The schema-version and plugin-policy gates moved
    /// to `ProjectConfig::validate` with their fields (SPEC §7).
    pub fn validate(&self) -> Result<()> {
        if self.worktree_dir.starts_with('/') || self.worktree_dir.contains("..") {
            return Err(BallError::Other(format!(
                "invalid config: worktree_dir {:?} must be a relative path with no '..' segments",
                self.worktree_dir
            )));
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

    /// Resolved `review.pre_check` command, or `None` when no review
    /// gate is configured. Single seam `bl review` consults.
    pub fn review_pre_check(&self) -> Option<&str> {
        self.review.as_ref().and_then(|r| r.pre_check.as_deref())
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "config_schema_tests.rs"]
mod schema_tests;
