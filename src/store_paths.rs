//! Filesystem path resolution helpers for `Store`. Extracted from
//! `store.rs` so that the main module stays focused on the Store API.

use crate::error::{BallError, Result};
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

pub(crate) fn find_main_root(common_dir: &Path) -> Result<PathBuf> {
    let canon = fs::canonicalize(common_dir).unwrap_or_else(|_| common_dir.to_path_buf());
    canon
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| BallError::Other("could not find main repo root".to_string()))
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
}
