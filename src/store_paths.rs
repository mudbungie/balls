//! Filesystem path resolution helpers for `Store`. Extracted from
//! `store.rs` so that the main module stays focused on the Store API.

use crate::error::{BallError, Result};
use crate::git;
use std::fs;
use std::path::{Path, PathBuf};

/// Task directory resolution. Stealth mode honors an override file at
/// `.balls/local/tasks_dir`; non-stealth mode points at the state
/// worktree's `.balls/tasks/` (the canonical location — main's
/// `.balls/tasks` is a gitignored symlink to the same place).
pub(crate) fn resolve_tasks_dir(root: &Path) -> (PathBuf, bool) {
    let override_file = root.join(".balls/local/tasks_dir");
    if let Ok(s) = fs::read_to_string(&override_file) {
        let p = PathBuf::from(s.trim());
        if p.is_absolute() {
            return (p, true);
        }
    }
    (root.join(".balls/worktree/.balls/tasks"), false)
}

/// Generate a deterministic external path for stealth tasks.
///
/// Why `sha1` and not `git hash-object` (footprint audit, epic
/// bl-32f8): this is the one in-tree hash that the jq+git+ln line
/// genuinely cannot reach. Two independent disqualifiers:
///   1. This path is computed in *stealth / no-git mode* — the mode
///      whose entire point is operating outside a git repo. Deriving
///      the store location by shelling to git would require the very
///      context this mode is built to not need.
///   2. `git hash-object` returns `sha1("blob <len>\0" + content)`,
///      not `sha1(content)`. The value differs, and this hash *is*
///      the store directory: changing it silently orphans every
///      existing stealth store. It is a persisted on-disk contract.
/// So `sha1`+`hex` is a deliberate, bounded kept exception — same
/// shape as the plugin/libc line: small, well-understood, clearly
/// demarcated, with no primitive substitute. The only remaining
/// lever is vendoring a ~75-line SHA-1 (drops the RustCrypto chain,
/// keeps the exact values); deferred as an explicit decision because
/// it trades a vetted crate for a hand-maintained crypto primitive.
pub(crate) fn stealth_tasks_dir(root: &Path) -> PathBuf {
    use sha1::{Digest, Sha1};
    let root_str = root.to_string_lossy();
    let hash = hex::encode(Sha1::digest(root_str.as_bytes()));
    let base = dirs_base(&hash);
    PathBuf::from(base).join("tasks")
}

pub(crate) fn dirs_base(hash: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        format!("{}/.local/share/balls/{}", home, &hash[..12])
    } else {
        format!("/tmp/balls-stealth-{}", &hash[..12])
    }
}

/// Gate for `discover_git`: confirm `from` is inside a git repo. A
/// bare hub has no work tree, so `rev-parse --show-toplevel` (used by
/// `git_root`) fails there — but it is still a git repo whose gitdir
/// parent is the main root. Tolerate that; only a genuine non-git dir
/// keeps the `NotARepo` error so `discover` falls back to no-git
/// discovery. (bl-8cf7)
pub(crate) fn require_git_repo(from: &Path) -> Result<()> {
    match git::git_root(from) {
        Ok(_) => Ok(()),
        Err(_) if crate::bare_squash::is_bare_repo(from).unwrap_or(false) => Ok(()),
        Err(e) => Err(e),
    }
}

pub(crate) fn find_main_root(common_dir: &Path) -> Result<PathBuf> {
    let canon = fs::canonicalize(common_dir).unwrap_or_else(|_| common_dir.to_path_buf());
    canon
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| BallError::Other("could not find main repo root".to_string()))
}

/// Walk up from `from` looking for `.balls/config.json` to locate
/// the project root when no git repo is available (no-git mode).
pub(crate) fn find_balls_root(from: &Path) -> Result<PathBuf> {
    let mut cur = fs::canonicalize(from).unwrap_or_else(|_| from.to_path_buf());
    let mut searched = Vec::new();
    loop {
        searched.push(cur.clone());
        if cur.join(".balls/config.json").exists() {
            return Ok(cur);
        }
        if !cur.pop() {
            return Err(BallError::no_balls_on_walk(searched));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_base_no_home() {
        let saved = std::env::var("HOME").ok();
        std::env::remove_var("HOME");
        let result = dirs_base("abcdef123456789");
        if let Some(h) = saved {
            std::env::set_var("HOME", h);
        }
        assert!(result.starts_with("/tmp/balls-stealth-"));
    }

    #[test]
    fn find_balls_root_reports_the_walked_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let start = dir.path().join("a/b/c");
        fs::create_dir_all(&start).unwrap();
        let err = find_balls_root(&start).unwrap_err();
        match err {
            BallError::NotInitialized(crate::error::NotInitKind::NoBallsOnWalk(searched)) => {
                // First entry is the (canonicalized) start dir; the walk
                // ends at the filesystem root.
                assert!(searched.len() >= 4);
                assert!(searched.first().unwrap().ends_with("a/b/c"));
                assert_eq!(searched.last().unwrap(), Path::new("/"));
            }
            other => panic!("expected NoBallsOnWalk, got {other}"),
        }
    }
}
