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
//!
//! Fetches ask for `--filter=blob:none` so we pay for commit metadata
//! only. Older servers that do not speak partial clone reject the
//! filter outright; a recognized rejection triggers one unfiltered
//! retry so those deployments still resolve, paying full blobs (bl-dbe5).

use crate::git;
use crate::task::Task;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

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
        if !fetch_with_fallback(url, &mut |filter| fetch_origin(&dir, filter)) {
            eprintln!("note: could not refresh code-refs for `{url}`; using warm cache");
        }
        return Some(dir);
    }
    if fetch_with_fallback(url, &mut |filter| clone_bare(&dir, url, filter)) {
        return Some(dir);
    }
    let _ = fs::remove_dir_all(&dir);
    warn_unreachable(url);
    None
}

/// Run `op(true)` (a `--filter=blob:none` fetch or clone). When the
/// server rejects the filter — an older deployment that does not speak
/// partial clone — retry `op(false)` once unfiltered and note the
/// degraded fetch. A genuinely-unreachable URL fails the first attempt
/// without a filter-rejection signature, so it pays no second
/// round-trip (bl-dbe5). Returns whether the cache is now usable.
///
/// `op` is a trait object rather than a generic so the orchestration
/// compiles to a single function — the filter-rejection arm is only
/// reachable from a test injecting a synthetic failure, and a generic
/// would split that arm across monomorphizations the coverage tool
/// cannot reassemble.
fn fetch_with_fallback(url: &str, op: &mut dyn FnMut(bool) -> std::io::Result<Output>) -> bool {
    match classify(op(true)) {
        Attempt::Ok => true,
        Attempt::FilterRejected => {
            let recovered = matches!(classify(op(false)), Attempt::Ok);
            if recovered {
                note_unfiltered(url);
            }
            recovered
        }
        Attempt::Failed => false,
    }
}

/// Classification of one `git` invocation's outcome.
#[derive(Debug, PartialEq, Eq)]
enum Attempt {
    Ok,
    FilterRejected,
    Failed,
}

fn classify(result: std::io::Result<Output>) -> Attempt {
    match result {
        Ok(out) if out.status.success() => Attempt::Ok,
        Ok(out) if is_filter_rejection(&out.stderr) => Attempt::FilterRejected,
        _ => Attempt::Failed,
    }
}

/// Recognize the stderr of a server that does not speak partial clone.
/// Modern git degrades a missing filter capability to a warning and
/// exit 0, so this only fires for older deployments (pre-7.x Bitbucket
/// Server, gitea before partial-clone, plain `git daemon`) that fail
/// the fetch outright. Their phrasing varies, so match the `filter`
/// signal alongside any of the known refusal forms.
fn is_filter_rejection(stderr: &[u8]) -> bool {
    let s = String::from_utf8_lossy(stderr).to_lowercase();
    s.contains("filter")
        && (s.contains("not supported")
            || s.contains("not recognized")
            || s.contains("not advertised"))
}

fn fetch_origin(dir: &Path, with_filter: bool) -> std::io::Result<Output> {
    let mut cmd = git::clean_git_command(dir);
    cmd.args(["fetch", "--quiet"]);
    if with_filter {
        cmd.arg("--filter=blob:none");
    }
    cmd.arg("origin").output()
}

fn clone_bare(dir: &Path, url: &str, with_filter: bool) -> std::io::Result<Output> {
    let parent = dir.parent().expect("cache_dir_for always has a parent");
    let _ = fs::create_dir_all(parent);
    let mut cmd = git::clean_git_command(parent);
    cmd.args(["clone", "--bare", "--quiet"]);
    if with_filter {
        cmd.arg("--filter=blob:none");
    }
    cmd.arg(url).arg(dir).output()
}

fn note_unfiltered(url: &str) {
    eprintln!(
        "note: `{url}` does not support partial clone; \
         code-refs cache fetched with full blobs"
    );
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
