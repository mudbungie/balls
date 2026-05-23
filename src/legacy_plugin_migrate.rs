//! Code-branch cleanup for repos predating the bl-8a9a unified state
//! checkout.
//!
//! The pre-bl-8a9a layout committed `.balls/plugins/*.json` to the
//! code branch; bl-8a9a moves the *filesystem* files into the state
//! checkout (`absorb_plugins_dir`) and points `.balls/plugins` at it
//! via a symlink, but never `git rm`s the legacy index entries or
//! refreshes the workspace's `.gitignore`. The shipping bl-8a9a code
//! therefore leaves a migrated workspace dirty: the code branch shows
//! `.balls/plugins/*.json` as deleted and `.balls/plugins` as a
//! typechanged symlink, and `.balls/state-repo` is untracked — a
//! plain `git add -A` bakes it in as an embedded git repo (bl-de57).
//!
//! This module is the missing migration commit. On every cold and
//! warm discover it: `git rm --cached`s any `.balls/plugins/*` paths
//! still tracked in `HEAD`, refreshes `.gitignore` with the unified
//! runtime paths (via `runtime_paths`, the single source of truth),
//! and commits the result on the code branch. Idempotent: a workspace
//! with neither legacy index entries nor missing gitignore lines
//! produces no commit. A bare hub has no working tree to commit from
//! and is skipped — the bare hub gets the migration commit pushed in
//! from a downstream clone instead.

use crate::error::Result;
use crate::{bare_squash, git, gitignore};
use std::path::{Path, PathBuf};

const COMMIT_MSG: &str = "balls: migrate plugins to state checkout";

/// Migration entry-point: clean the workspace's code branch of legacy
/// committed plugin files and refresh `.gitignore`. A no-op on a bare
/// hub, a non-git dir, or a workspace with an unborn HEAD. A
/// fully-migrated workspace exits without touching the index — the
/// read-only fast path keeps `bl`'s discover phase out of the
/// `index.lock` race a parallel `bl` storm otherwise creates.
pub(crate) fn run(root: &Path) -> Result<()> {
    if !shape_supports_commit(root) {
        return Ok(());
    }
    let legacy = legacy_paths_in_head(root)?;
    let gitignore_missing = gitignore_missing_entries(root);
    if legacy.is_empty() && gitignore_missing.is_empty() {
        return Ok(());
    }
    if !legacy.is_empty() {
        let refs: Vec<&Path> = legacy.iter().map(PathBuf::as_path).collect();
        git::git_rm_cached(root, &refs)?;
    }
    if !gitignore_missing.is_empty() {
        gitignore::ensure_main_gitignore(root, false)?;
        git::git_add(root, &[Path::new(".gitignore")])?;
    }
    git::git_commit(root, COMMIT_MSG)?;
    Ok(())
}

/// Whether the workspace is shaped so a migration commit makes sense:
/// a non-bare repo with at least one existing commit on HEAD.
fn shape_supports_commit(root: &Path) -> bool {
    if !root.join(".git").exists() {
        return false;
    }
    if bare_squash::is_bare_repo(root).unwrap_or(false) {
        return false;
    }
    git::git_has_any_commits(root)
}

/// Every path under `.balls/plugins/` still tracked in `HEAD`. These
/// are the legacy committed plugin config files (`*.json` + `.gitkeep`)
/// that `absorb_plugins_dir` moved off disk without unstaging.
fn legacy_paths_in_head(root: &Path) -> Result<Vec<PathBuf>> {
    let out = git::run_git_in(
        root,
        &["ls-tree", "-r", "--name-only", "HEAD", ".balls/plugins/"],
    )?;
    if !out.status.success() {
        // `.balls/plugins/` was never tracked on this branch's HEAD.
        return Ok(Vec::new());
    }
    let s = String::from_utf8_lossy(&out.stdout);
    Ok(s.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect())
}

/// Runtime-path entries the workspace `.gitignore` is missing.
/// A read-only check: no file is written here. An absent `.gitignore`
/// counts every entry as missing — the migration path will create it.
fn gitignore_missing_entries(root: &Path) -> Vec<&'static str> {
    let content = std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let present: std::collections::HashSet<&str> = content.lines().map(str::trim).collect();
    crate::runtime_paths::gitignore_paths(false)
        .into_iter()
        .filter(|p| !present.contains(*p))
        .collect()
}

#[cfg(test)]
#[path = "legacy_plugin_migrate_tests.rs"]
mod tests;
