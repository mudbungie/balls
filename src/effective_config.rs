//! Three-layer precedence merger per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §6.5.
//!
//! For each layered field, the effective value is:
//!
//! ```text
//! effective = clone.json field     if present
//!          ?? repo.json field      if present
//!          ?? project.json field   if present
//!          ?? built-in default
//! ```
//!
//! Read once at the start of every `bl` invocation; cached for
//! that invocation. No per-call override (SPEC §6.5).
//!
//! This module is the *only* surface the binary should consult
//! when reading a layered field. Reaching into `RepoJson` or
//! `CloneJson` directly bypasses the precedence rule and
//! reintroduces the per-call-overlay bug class.

use crate::clone_json::CloneJson;
use crate::layered_fields::{Integrate, ReviewBlock};
use crate::project_config::ProjectConfig;
use crate::repo_json::{
    RepoJson, DEFAULT_AUTO_FETCH_ON_READY, DEFAULT_PROTECTED_MAIN, DEFAULT_REQUIRE_REMOTE,
};

/// Resolved view of one `bl` invocation's effective config. What
/// the running binary consults instead of any individual layer's
/// struct.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub integrate: Integrate,
    pub review: ReviewBlock,
    pub require_remote_on_claim: bool,
    pub require_remote_on_review: bool,
    pub require_remote_on_close: bool,
    pub auto_fetch_on_ready: bool,
    pub stale_threshold_seconds: u64,
    pub worktree_dir: Option<String>,
    pub protected_main: bool,
}

impl EffectiveConfig {
    /// Merge the three layers per §6.5 precedence. `clone` may be
    /// `None` (the clone.json file is absent for clones with no
    /// per-checkout overrides — the common case).
    #[must_use]
    pub fn resolve(
        project: &ProjectConfig,
        repo: &RepoJson,
        clone: Option<&CloneJson>,
    ) -> Self {
        Self {
            integrate: pick(
                clone.and_then(|c| c.integrate.as_ref()),
                repo.integrate.as_ref(),
                project.integrate.as_ref(),
                Integrate::default,
            ),
            review: pick(
                clone.and_then(|c| c.review.as_ref()),
                repo.review.as_ref(),
                project.review.as_ref(),
                ReviewBlock::default,
            ),
            require_remote_on_claim: pick_bool(
                clone.and_then(|c| c.require_remote_on_claim),
                repo.require_remote_on_claim,
                project.require_remote_on_claim,
                DEFAULT_REQUIRE_REMOTE,
            ),
            require_remote_on_review: pick_bool(
                clone.and_then(|c| c.require_remote_on_review),
                repo.require_remote_on_review,
                project.require_remote_on_review,
                DEFAULT_REQUIRE_REMOTE,
            ),
            require_remote_on_close: pick_bool(
                clone.and_then(|c| c.require_remote_on_close),
                repo.require_remote_on_close,
                project.require_remote_on_close,
                DEFAULT_REQUIRE_REMOTE,
            ),
            // Repo-only fields below — no project layer in §6.5.
            // The function takes only two layers (clone + repo) plus
            // the default; project is excluded by the §6.5 table.
            auto_fetch_on_ready: pick_bool(
                clone.and_then(|c| c.auto_fetch_on_ready),
                Some(repo.auto_fetch_on_ready),
                None,
                DEFAULT_AUTO_FETCH_ON_READY,
            ),
            stale_threshold_seconds: clone
                .and_then(|c| c.stale_threshold_seconds)
                .unwrap_or(repo.stale_threshold_seconds),
            worktree_dir: clone
                .and_then(|c| c.worktree_dir.clone())
                .or_else(|| repo.worktree_dir.clone()),
            protected_main: pick_bool(
                clone.and_then(|c| c.protected_main),
                Some(repo.protected_main),
                None,
                DEFAULT_PROTECTED_MAIN,
            ),
        }
    }
}

/// `clone ?? repo ?? project ?? default()` for clonable layered
/// block types. The function takes references and clones on hit so
/// the merger keeps `repo` and `project` borrowed (the caller
/// owns the structs).
fn pick<T: Clone, F: FnOnce() -> T>(
    clone: Option<&T>,
    repo: Option<&T>,
    project: Option<&T>,
    default: F,
) -> T {
    clone
        .or(repo)
        .or(project)
        .cloned()
        .unwrap_or_else(default)
}

/// `clone ?? repo ?? project ?? default` for bool fields. Same
/// precedence; takes owned Options because Copy types don't need
/// the ref dance.
fn pick_bool(
    clone: Option<bool>,
    repo: Option<bool>,
    project: Option<bool>,
    default: bool,
) -> bool {
    clone.or(repo).or(project).unwrap_or(default)
}

#[cfg(test)]
#[path = "effective_config_tests.rs"]
mod tests;
