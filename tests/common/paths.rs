//! Layout-aware path-resolution helpers for integration tests.
//!
//! All XDG path math runs against the per-thread test HOME via
//! [`super::test_home_path`] — no process-env mutation (bl-bfa8 /
//! `project_test_global_env_race`). Each helper probes the same
//! sequence `Store::discover` walks (`xdg_discover.rs`) so that a
//! legacy `bl init` (Phase 1B-2 not yet flipped) resolves to the
//! same in-repo paths the binary actually writes, and a future XDG
//! `bl init` resolves to the tracker checkout. Split out of
//! `common/mod.rs` for the 300-line cap; re-exported there.

#![allow(dead_code)]

use balls::git::clean_git_command;
use std::path::{Path, PathBuf};

use super::test_home_path;

/// `XdgBases` rooted at this thread's test HOME — the building block
/// every other resolver in this module composes against.
pub fn test_xdg_bases() -> balls::xdg_paths::XdgBases {
    balls::xdg_paths::XdgBases::with(&test_home_path(), None, None, None)
}

/// Origin URL configured on the clone, or `None` if no origin is set.
fn origin_url(repo_root: &Path) -> Option<String> {
    let out = clean_git_command(repo_root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// XDG tracker checkout path for the repo's origin, or `None` if
/// origin is unset. Materialization is not checked here.
fn xdg_tracker_checkout(repo_root: &Path) -> Option<PathBuf> {
    let url = origin_url(repo_root)?;
    let enc = balls::encoding::percent_encode_component(&balls::encoding::canonicalize_origin(&url));
    Some(balls::xdg_paths::own_tracker_checkout(&test_xdg_bases(), &enc))
}

/// True when this clone's XDG state (tracker checkout) is materialized
/// — the same probe `Store::discover` makes before preferring XDG.
fn is_xdg(repo_root: &Path) -> bool {
    xdg_tracker_checkout(repo_root).is_some_and(|p| p.join(".git").exists())
}

/// `clone.json.tasks_dir` if the repo is a stealth XDG clone, else
/// `None`. Env-free path arithmetic, same as the other resolvers.
fn discover_stealth_tasks(repo_root: &Path) -> Option<PathBuf> {
    let bases = test_xdg_bases();
    let canon = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let nested = balls::encoding::nested_clone_path(&canon);
    let cj_path = balls::xdg_paths::clone_json_path(&bases, &nested);
    let cj = balls::clone_json::CloneJson::read_optional(&cj_path).ok().flatten()?;
    cj.stealth.then(|| cj.tasks_dir.map(PathBuf::from)).flatten()
}

/// Resolve the active state-repo (tracker checkout) path for a repo.
///
/// XDG tracker checkout when materialized; else `<repo>/.balls/state-repo`
/// (legacy unified state-checkout). `None` for stealth clones.
pub fn discover_state_repo(repo_root: &Path) -> Option<PathBuf> {
    if discover_stealth_tasks(repo_root).is_some() {
        return None;
    }
    if let Some(xdg) = xdg_tracker_checkout(repo_root) {
        if xdg.join(".git").exists() {
            return Some(xdg);
        }
    }
    Some(repo_root.join(".balls/state-repo"))
}

/// Tasks dir for a clone.
///
/// Stealth → `clone.json.tasks_dir`. XDG → tracker checkout's
/// `.balls/tasks`. Legacy → `<repo>/.balls/tasks`, the symlink the
/// legacy unified-state-checkout layout exposes at the clone root.
pub fn discover_tasks_dir(repo_root: &Path) -> PathBuf {
    if let Some(td) = discover_stealth_tasks(repo_root) {
        return td;
    }
    if let Some(xdg) = xdg_tracker_checkout(repo_root) {
        if xdg.join(".git").exists() {
            return xdg.join(".balls/tasks");
        }
    }
    repo_root.join(".balls/tasks")
}

/// Worktree path for a task on this clone — the directory `bl claim`
/// worktree-adds into.
///
/// XDG: `~/.local/state/balls/worktrees/<nested-clone-path>/<id>/`.
/// Legacy: `<repo>/.balls-worktrees/<id>`. Mirrors `Store::worktrees_root`.
///
/// `id` is `AsRef<str>` so the bulk replacement of legacy
/// `<repo>.join(".balls-worktrees").join(&id)` carries over without
/// adding/dropping a `&` per call site.
pub fn worktree_path(repo_root: &Path, id: impl AsRef<str>) -> PathBuf {
    let id = id.as_ref();
    if is_xdg(repo_root) {
        let canon = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
        let nested = balls::encoding::nested_clone_path(&canon);
        let per = balls::xdg_paths::PerClonePaths::new(&test_xdg_bases(), &nested);
        return per.worktree_for(id);
    }
    repo_root.join(".balls-worktrees").join(id)
}

/// Per-clone XDG paths bundle for a non-stealth clone, or `None` for
/// stealth. Computed unconditionally even when XDG is not yet
/// materialized — Phase 1B-2+ tests assert on these paths once the
/// flip lands.
pub fn per_clone_paths(repo_root: &Path) -> Option<balls::xdg_paths::PerClonePaths> {
    if discover_stealth_tasks(repo_root).is_some() {
        return None;
    }
    let canon = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let nested = balls::encoding::nested_clone_path(&canon);
    Some(balls::xdg_paths::PerClonePaths::new(&test_xdg_bases(), &nested))
}

/// XDG claims directory (Phase 1B-2+ tests). Panics on stealth.
pub fn claims_dir(repo_root: &Path) -> PathBuf {
    per_clone_paths(repo_root).expect("non-stealth repo for claims_dir").claims
}

/// XDG per-clone lock directory.
pub fn lock_dir(repo_root: &Path) -> PathBuf {
    per_clone_paths(repo_root).expect("non-stealth repo for lock_dir").locks
}

/// XDG per-clone plugin-auth directory (the new home for what legacy
/// stored under `.balls/local/plugins`).
pub fn plugins_auth_dir(repo_root: &Path) -> PathBuf {
    per_clone_paths(repo_root)
        .expect("non-stealth repo for plugins_auth_dir")
        .plugins_auth
}

/// XDG per-clone worktrees root — `worktree_path(repo, id)` joins on
/// this for a specific task.
pub fn worktrees_dir(repo_root: &Path) -> PathBuf {
    per_clone_paths(repo_root)
        .expect("non-stealth repo for worktrees_dir")
        .worktrees
}

/// XDG per-clone cache directory — `~/.cache/balls/<nested>/`, where
/// runtime markers like `last_fetch` (bl-5814) live. Mirrors
/// `Store::cache_dir`; used by integration tests that need to check
/// for a marker file directly.
pub fn cache_dir(repo_root: &Path) -> PathBuf {
    let canon = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let nested = balls::encoding::nested_clone_path(&canon);
    test_xdg_bases().cache_root().join(nested)
}

/// Path to the `last_fetch` marker `bl ready --auto-fetch` writes.
/// Convenience wrapper over [`cache_dir`].
pub fn cache_last_fetch(repo_root: &Path) -> PathBuf {
    cache_dir(repo_root).join("last_fetch")
}
