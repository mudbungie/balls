//! Single source of truth for balls' internal runtime paths (bl-228d).
//!
//! `bl` materializes several paths inside a workspace that are its own
//! state, not deliverables: the per-clone `.balls/local/` dir, the
//! state checkout at `.balls/state-repo/`, its `.balls/tasks` and
//! `.balls/plugins` convenience symlinks, the `--resolve-remote`
//! code-refs cache, and the task worktrees under `.balls-worktrees/`.
//! Two consumers must keep every one of them off the workspace's
//! integration branch, and each used to hand-maintain its own list:
//!
//!   - `gitignore::ensure_main_gitignore` writes them to the workspace
//!     checkout's `.gitignore` at `bl init`;
//!   - `review_safety` strips them from the `bl review` squash and
//!     rejects any squash that carries one in anyway.
//!
//! This table is the one declaration both consumers derive from: a new
//! runtime path is a single row here, and the invariant is structural.
//! `.balls/config.json` is *not* here — it is a committed,
//! workspace-owned deliverable under the unified model.

/// One balls-internal runtime path, with the facts that decide how
/// each consumer treats it.
pub(crate) struct RuntimePath {
    /// Path, relative to the repo root.
    pub rel: &'static str,
    /// Whether the path exists in stealth mode. The state checkout and
    /// its symlinks (`.balls/tasks`, `.balls/state-repo`,
    /// `.balls/plugins`) never exist in stealth mode, so a stealth
    /// `.gitignore` omits them.
    pub in_stealth: bool,
    /// Whether the `bl review` squash backstop covers this path —
    /// `add_user_changes` unstages it and `commit_touches_runtime`
    /// rejects a squash carrying it. True for the `.balls/` state
    /// paths a `work/<id>` tree can plausibly track; false for
    /// `.balls-worktrees` (the *parent* of work worktrees, never a
    /// subpath of one) and `.balls/plugins` (a gitignored symlink).
    pub backstop: bool,
}

/// The canonical runtime-path table. Adding a sidecar is one row, and
/// every consumer picks it up.
pub(crate) const RUNTIME_PATHS: &[RuntimePath] = &[
    RuntimePath { rel: ".balls/local", in_stealth: true, backstop: true },
    RuntimePath { rel: ".balls/tasks", in_stealth: false, backstop: true },
    RuntimePath { rel: ".balls/state-repo", in_stealth: false, backstop: true },
    RuntimePath { rel: ".balls/plugins", in_stealth: false, backstop: false },
    RuntimePath { rel: ".balls/code-refs", in_stealth: true, backstop: true },
    RuntimePath { rel: ".balls-worktrees", in_stealth: true, backstop: false },
];

/// Paths the `bl review` squash backstop strips and rejects — the
/// `backstop` subset of the table. Consumed by `review_safety`.
pub(crate) fn backstop_paths() -> Vec<&'static str> {
    RUNTIME_PATHS
        .iter()
        .filter(|p| p.backstop)
        .map(|p| p.rel)
        .collect()
}

/// Paths `bl init` writes to the workspace checkout's `.gitignore`:
/// every runtime path, dropping the ones absent under stealth.
pub(crate) fn gitignore_paths(is_stealth: bool) -> Vec<&'static str> {
    RUNTIME_PATHS
        .iter()
        .filter(|p| p.in_stealth || !is_stealth)
        .map(|p| p.rel)
        .collect()
}

#[cfg(test)]
#[path = "runtime_paths_tests.rs"]
mod tests;
