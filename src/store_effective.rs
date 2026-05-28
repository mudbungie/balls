//! The `EffectiveConfig` accessor for `Store`, plus the
//! `integration_branch[_for]` resolution helpers.
//!
//! Phase 1B-6 (bl-c122) retired the temporary `Config`-shaped XDG
//! synthesizer that propped up legacy call sites through Phase 1B.
//! Every read of a layered field now routes through here, returning
//! an `EffectiveConfig` regardless of layout:
//!
//! - **XDG**: resolved per SPEC §6.5 from `repo.json` +
//!   `project.json` + `clone.json`.
//! - **Legacy**: the on-disk `.balls/config.json` (`Config`) is
//!   loaded and *adapted* into the post-XDG `EffectiveConfig` shape.
//!   The translation runs only until `bl migrate` flips the clone to
//!   XDG; after that the legacy file is gone and the adapter never
//!   runs.
//!
//! Note `target_branch` is intentionally absent from
//! `EffectiveConfig` — SPEC §6.7 removed the repo-level field. The
//! Legacy-only fallback survives via [`explicit_repo_target_branch`]
//! and [`integration_branch_for`], not via the EffectiveConfig shape.

use crate::clone_json::CloneJson;
use crate::config::{Config, DeliveryMode};
use crate::effective_config::EffectiveConfig;
use crate::error::Result;
use crate::git;
use crate::layered_fields::{Integrate, IntegrateMode, ReviewBlock};
use crate::project_config::ProjectConfig;
use crate::repo_json::RepoJson;
use crate::store::{Layout, Store};
use std::path::Path;

/// Read the EffectiveConfig view for this store. XDG resolves
/// `repo.json` + `project.json` + `clone.json` via the §6.5 merger;
/// Legacy adapts the on-disk `.balls/config.json` into the new shape.
pub(crate) fn load(store: &Store) -> Result<EffectiveConfig> {
    match store.layout {
        Layout::Xdg => load_xdg(
            &store.config_path(),
            &store.project_config_path(),
            store.clone_json(),
        ),
        Layout::Legacy => Config::load(&store.config_path()).map(|c| from_legacy(&c)),
    }
}

fn load_xdg(
    repo_path: &Path,
    project_path: &Path,
    clone: Option<&CloneJson>,
) -> Result<EffectiveConfig> {
    let repo = RepoJson::read_or_default(repo_path)?;
    let project = ProjectConfig::resolve(project_path, Path::new("")).unwrap_or_default();
    Ok(EffectiveConfig::resolve(&project, &repo, clone))
}

/// Adapt a legacy `Config` (pre-XDG `.balls/config.json`) into the
/// post-XDG `EffectiveConfig` shape. The pre-XDG `target_branch`
/// field is intentionally dropped from this view (SPEC §6.7); the
/// Legacy-only fallback is preserved out-of-band by
/// [`explicit_repo_target_branch`].
fn from_legacy(cfg: &Config) -> EffectiveConfig {
    EffectiveConfig {
        integrate: Integrate {
            mode: match cfg.delivery_mode() {
                DeliveryMode::LocalSquash => IntegrateMode::Direct,
                DeliveryMode::Deferred => IntegrateMode::ForgePr,
            },
        },
        review: ReviewBlock {
            gate_command: cfg.review_pre_check().map(str::to_string),
        },
        require_remote_on_claim: cfg.require_remote_on_claim,
        require_remote_on_review: cfg.require_remote_on_review,
        require_remote_on_close: cfg.require_remote_on_close,
        auto_fetch_on_ready: cfg.auto_fetch_on_ready,
        stale_threshold_seconds: cfg.stale_threshold_seconds,
        worktree_dir: Some(cfg.worktree_dir.clone()),
        protected_main: cfg.protected_main,
    }
}

/// Resolve the integration branch for a task with the given optional
/// per-task `target_branch`. Single seam every consumer routes
/// through. Precedence:
///   `task.target_branch ?? legacy-repo.target_branch ?? HEAD@root`
/// The middle layer applies under Legacy only — SPEC §6.7 removed
/// the repo-level field from the XDG schema, so under XDG the chain
/// collapses to `task.target_branch ?? HEAD@root`.
pub(crate) fn integration_branch_for(
    store: &Store,
    task_target: Option<&str>,
) -> Result<String> {
    if let Some(b) = task_target {
        return Ok(b.to_string());
    }
    if let Some(b) = explicit_repo_target_branch(store)? {
        return Ok(b);
    }
    git::git_current_branch(&store.root)
}

/// The repo-level `target_branch` override, or `None`. Always `None`
/// under XDG (SPEC §6.7); under Legacy returns the parsed
/// `Config::target_branch`. Used by `bl review` to validate that
/// deferred mode has an unambiguous PR base.
pub(crate) fn explicit_repo_target_branch(store: &Store) -> Result<Option<String>> {
    match store.layout {
        Layout::Xdg => Ok(None),
        Layout::Legacy => Ok(Config::load(&store.config_path())?.target_branch),
    }
}

#[cfg(test)]
#[path = "store_effective_tests.rs"]
mod tests;
