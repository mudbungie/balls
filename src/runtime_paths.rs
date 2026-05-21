//! Single source of truth for balls' internal runtime paths (bl-228d).
//!
//! `bl` materializes several paths inside a project that are its own
//! state, not deliverables: the per-clone `.balls/local/` dir, the
//! state worktree at `.balls/worktree/`, the `--resolve-remote`
//! code-refs cache, the `master_url` hub clone at `.balls/state-repo/`,
//! and the task worktrees under `.balls-worktrees/`. Two consumers
//! must keep every one of them off the project's integration branch,
//! and each used to hand-maintain its own list:
//!
//!   - `store_init::ensure_main_gitignore` writes them to the main
//!     checkout's `.gitignore` at `bl init`;
//!   - `review_safety` strips them from the `bl review` squash and
//!     rejects any squash that carries one in anyway.
//!
//! The two lists overlapped but were not equal, kept aligned only by a
//! `// Kept in sync` comment — so every new sidecar had to be
//! remembered in both places, and more than one (`.balls/code-refs`)
//! shipped half-wired and needed a retrofit. This table is the one
//! declaration both consumers now derive from: a new runtime path is a
//! single row here, and the invariant is structural, not aspirational.

/// One balls-internal runtime path, with the two facts that decide how
/// each consumer treats it.
pub(crate) struct RuntimePath {
    /// Path, relative to the repo root.
    pub rel: &'static str,
    /// Whether the path exists in stealth mode. `.balls/tasks` and
    /// `.balls/worktree` make up the state worktree, which stealth
    /// mode never creates, so `ensure_main_gitignore` omits them from
    /// a stealth `.gitignore`. Every other path exists in all modes.
    pub in_stealth: bool,
    /// Whether the `bl review` squash backstop covers this path —
    /// `add_user_changes` unstages it and `commit_touches_runtime`
    /// rejects a squash carrying it. True for the `.balls/` state
    /// paths, which a `work/<id>` tree can plausibly track. False for
    /// `.balls-worktrees`: it is the *parent* of the work worktrees,
    /// never a subpath of one, so a squash of `work/<id>` cannot reach
    /// it — gitignoring it is the whole defense.
    pub backstop: bool,
}

/// The canonical runtime-path table. Adding a sidecar is one row, and
/// both consumers pick it up.
pub(crate) const RUNTIME_PATHS: &[RuntimePath] = &[
    RuntimePath { rel: ".balls/local", in_stealth: true, backstop: true },
    RuntimePath { rel: ".balls/tasks", in_stealth: false, backstop: true },
    RuntimePath { rel: ".balls/worktree", in_stealth: false, backstop: true },
    RuntimePath { rel: ".balls/code-refs", in_stealth: true, backstop: true },
    RuntimePath { rel: ".balls/state-repo", in_stealth: true, backstop: true },
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

/// Paths `bl init` writes to the main checkout's `.gitignore`: every
/// runtime path, dropping the ones that do not exist under stealth.
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
