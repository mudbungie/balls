//! Cross-repo delivery resolution via cached fetches (bl-f37b).
//!
//! When `delivered_in` cannot be resolved against the local integration
//! branch — because the local clone is a sibling code repo, or a bare
//! task hub with no code at all — `delivered_repo` names the repo whose
//! history holds the squash sha. This module materializes a balls-owned
//! bare git cache under `.balls/code-refs/<hash>.git`, fetches its
//! refs, and re-runs the `[bl-xxxx]` tag scan against the fetched
//! history.
//!
//! Soft-fail on every remote failure: warn once and return `None`. A
//! failed fetch is a degraded read, not a broken command — the master
//! hub's `master_url` is allowed to hard-fail because nothing else
//! works without it (bl-dcd3); here the cost of failure is just an
//! unresolvable sha, and `bl show` keeps working without provenance.

use crate::git;
use crate::task::Task;
use std::fs;
use std::path::{Path, PathBuf};

/// Relative path (from the repo root) of the code-refs cache. Each
/// `delivered_repo` URL maps to a SHA-1-named subdirectory so the cache
/// is filesystem-safe regardless of URL shape and shareable across
/// tasks that resolve through the same repo.
pub(crate) const CODE_REFS_REL: &str = ".balls/code-refs";

/// Try to resolve `[bl-id]` against the cached clone of
/// `delivered_repo`. Returns the sha when the fetch + scan succeed, or
/// `None` if anything went wrong (URL unfetchable, tag not found, git
/// failure). `--all` covers the lookup against whatever default branch
/// the remote checks out — we do not assume the remote's integration
/// branch is named the same as the local one.
pub fn resolve(repo_root: &Path, delivered_repo: &str, task: &Task) -> Option<String> {
    let cache = ensure_cache(repo_root, delivered_repo)?;
    let tag = format!("[{}]", task.id);
    let out = git::clean_git_command(&cache)
        .args(["log", "-1", "--format=%H", "--all", "-F", "--grep", &tag])
        .output()
        .ok()?;
    // `git log --grep` exits 0 with empty stdout when no commit
    // matches, so an empty sha is the no-match case — no separate
    // nonzero-exit branch to check.
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

/// Materialize a bare git cache at `.balls/code-refs/<hash>.git` whose
/// `origin` is `url`, fetched with blob filtering so we pay for
/// commit metadata only. Idempotent: a warm cache is refreshed in
/// place; an offline refresh keeps serving the warm refs and emits a
/// note. A first-time fetch failure tears the half-built scaffold down
/// so the next attempt is a clean retry, matching the state-repo
/// hard-fail discipline (bl-dcd3) for the soft-fail tier.
pub(crate) fn ensure_cache(repo_root: &Path, url: &str) -> Option<PathBuf> {
    let dir = cache_dir_for(repo_root, url);
    if dir.join("HEAD").exists() {
        if !refresh(&dir) {
            eprintln!("note: could not refresh code-refs for `{url}`; using warm cache");
        }
        return Some(dir);
    }
    if clone_bare(&dir, url) {
        return Some(dir);
    }
    let _ = fs::remove_dir_all(&dir);
    warn_unreachable(url);
    None
}

fn refresh(dir: &Path) -> bool {
    git::clean_git_command(dir)
        .args(["fetch", "--quiet", "--filter=blob:none", "origin"])
        .status()
        .is_ok_and(|s| s.success())
}

fn clone_bare(dir: &Path, url: &str) -> bool {
    let parent = dir.parent().expect("cache_dir_for always has a parent");
    let _ = fs::create_dir_all(parent);
    git::clean_git_command(parent)
        .args([
            "clone",
            "--bare",
            "--quiet",
            "--filter=blob:none",
            url,
            &dir.to_string_lossy(),
        ])
        .status()
        .is_ok_and(|s| s.success())
}

fn warn_unreachable(url: &str) {
    eprintln!(
        "warning: could not fetch `{url}` for cross-repo delivery resolution; \
         `delivered_in_resolved` will be null"
    );
}

/// Cache directory for `url` under `repo_root`. SHA-1 of the URL keeps
/// the path filesystem-safe and stable across runs; the `.git` suffix
/// marks it as a git-dir so a stray `ls` is self-describing.
pub(crate) fn cache_dir_for(repo_root: &Path, url: &str) -> PathBuf {
    let hash = crate::hash::sha1_hex(url.as_bytes());
    repo_root.join(CODE_REFS_REL).join(format!("{hash}.git"))
}

#[cfg(test)]
#[path = "delivery_remote_tests.rs"]
mod tests;
