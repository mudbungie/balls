//! Filesystem path resolution helpers for `Store`. Extracted from
//! `store.rs` so that the main module stays focused on the Store API.

use crate::error::{BallError, Result};
use crate::git;
use std::fs;
use std::path::{Path, PathBuf};

/// The stealth/no-git tasks-dir override, if one is configured at
/// `.balls/local/tasks_dir`. `Some` ⇒ this repo is in stealth mode and
/// its task files live at the returned absolute path; `None` ⇒ a
/// normal repo whose state lives in `.balls/state-repo`.
pub(crate) fn stealth_tasks_override(root: &Path) -> Option<PathBuf> {
    let s = fs::read_to_string(root.join(".balls/local/tasks_dir")).ok()?;
    let p = PathBuf::from(s.trim());
    p.is_absolute().then_some(p)
}

/// Stealth/tasks-dir init leg. Returns the resolved external tasks
/// directory; stealth mode has no state branch and no state checkout.
pub(crate) fn init_stealth_tasks(
    repo_root: &Path,
    local_dir: &Path,
    tasks_dir: Option<String>,
) -> Result<PathBuf> {
    let ext = match tasks_dir {
        Some(td) => PathBuf::from(td),
        None => stealth_tasks_dir(repo_root),
    };
    fs::create_dir_all(&ext)?;
    fs::write(local_dir.join("tasks_dir"), ext.to_string_lossy().as_bytes())?;
    Ok(ext)
}

/// Generate a deterministic external path for stealth tasks.
///
/// The SHA-1 here is load-bearing on-disk state, not an implementation
/// choice (footprint audit bl-32f8). Two independent disqualifiers
/// against the jq+git+ln substitute (`git hash-object`):
///   1. This path is computed in *stealth / no-git mode* — the mode
///      whose entire point is operating outside a git repo. Deriving
///      the store location by shelling to git would require the very
///      context this mode is built to not need.
///   2. `git hash-object` returns `sha1("blob <len>\0" + content)`,
///      not `sha1(content)`. The value differs, and this hash *is*
///      the store directory: changing it silently orphans every
///      existing stealth store. It is a persisted on-disk contract.
///
/// Backed by the vendored `crate::hash::sha1_hex` (bl-cb4e), which
/// produces byte-identical output to `hex::encode(sha1::Sha1::digest)`
/// — the RustCrypto stack is no longer in the dependency tree, and
/// existing stealth stores keep resolving to the same directory.
pub(crate) fn stealth_tasks_dir(root: &Path) -> PathBuf {
    let root_str = root.to_string_lossy();
    let hash = crate::hash::sha1_hex(root_str.as_bytes());
    let base = dirs_base(&hash);
    PathBuf::from(base).join("tasks")
}

/// Base directory for a stealth/no-git store. Reads `HOME` once and
/// delegates to the pure `dirs_base_for`, which is where the actual
/// branch lives so it can be tested without an `env::remove_var` —
/// that mutation is process-global and races every concurrent test
/// that shells out to git (bl-bfa8).
pub(crate) fn dirs_base(hash: &str) -> String {
    dirs_base_for(std::env::var("HOME").ok().as_deref(), hash)
}

fn dirs_base_for(home: Option<&str>, hash: &str) -> String {
    match home {
        Some(home) => format!("{home}/.local/share/balls/{}", &hash[..12]),
        None => format!("/tmp/balls-stealth-{}", &hash[..12]),
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
    fn dirs_base_for_falls_back_to_tmp_without_home() {
        assert!(dirs_base_for(None, "abcdef123456789").starts_with("/tmp/balls-stealth-"));
    }

    #[test]
    fn dirs_base_for_uses_xdg_data_dir_under_home() {
        assert_eq!(
            dirs_base_for(Some("/home/u"), "abcdef123456789"),
            "/home/u/.local/share/balls/abcdef123456"
        );
    }

    #[test]
    fn dirs_base_reads_home_from_env() {
        // Wrapper coverage: the test environment always has HOME set,
        // so this exercises the env read + Some delegation without
        // mutating any process-global state.
        assert!(dirs_base("abcdef123456789").contains("/.local/share/balls/"));
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
