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

/// One balls-internal runtime path, with the facts that decide how
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
    /// Whether the path is internal state *only* under `master_url`
    /// (bl-ebae). `.balls/plugins` is a real, committed directory in a
    /// standalone repo, but the federated flip turns it into a symlink
    /// into the hub clone — so it is gitignored only when `federated`,
    /// and `bl remaster --detach` removes it again.
    pub federated_only: bool,
}

/// The canonical runtime-path table. Adding a sidecar is one row, and
/// every consumer picks it up.
pub(crate) const RUNTIME_PATHS: &[RuntimePath] = &[
    RuntimePath { rel: ".balls/local", in_stealth: true, backstop: true, federated_only: false },
    RuntimePath { rel: ".balls/tasks", in_stealth: false, backstop: true, federated_only: false },
    RuntimePath { rel: ".balls/worktree", in_stealth: false, backstop: true, federated_only: false },
    RuntimePath { rel: ".balls/code-refs", in_stealth: true, backstop: true, federated_only: false },
    RuntimePath { rel: ".balls/state-repo", in_stealth: true, backstop: true, federated_only: false },
    RuntimePath { rel: ".balls-worktrees", in_stealth: true, backstop: false, federated_only: false },
    // Federated-only: a squash backstop would wrongly strip a
    // standalone repo's deliverable plugin config, so `backstop` is
    // false — gitignoring it under `master_url` is the whole defense.
    RuntimePath { rel: ".balls/plugins", in_stealth: true, backstop: false, federated_only: true },
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

/// Paths `bl init` (and the `bl remaster` federated flip) write to the
/// main checkout's `.gitignore`: every runtime path, dropping the ones
/// that do not exist under stealth and the federated-only ones unless
/// `federated`.
pub(crate) fn gitignore_paths(is_stealth: bool, federated: bool) -> Vec<&'static str> {
    RUNTIME_PATHS
        .iter()
        .filter(|p| (p.in_stealth || !is_stealth) && (!p.federated_only || federated))
        .map(|p| p.rel)
        .collect()
}

/// Paths gitignored only under `master_url` — added by the remaster
/// federated flip and removed again on `bl remaster --detach`, where
/// `.balls/plugins` reverts to a real project-owned directory (bl-ebae).
pub(crate) fn federated_only_paths() -> Vec<&'static str> {
    RUNTIME_PATHS
        .iter()
        .filter(|p| p.federated_only)
        .map(|p| p.rel)
        .collect()
}

#[cfg(test)]
#[path = "runtime_paths_tests.rs"]
mod tests;
