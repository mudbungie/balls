//! Delivery-link resolution (SPEC §6).
//!
//! Each task carries a `delivered_in: Option<String>` hint pointing at
//! the squash-merge commit on main. Ground truth is the `[bl-xxxx]`
//! tag embedded in the commit message — the hint is a cache.
//!
//! On read, `resolve` verifies the hint is still reachable from main
//! *and* still contains the tag. If either check fails, it falls back
//! to a tag scan on main. Survives rebase, amend, cherry-pick, and
//! filter-branch because the tag travels with the commit.

use crate::git;
use crate::task::Task;
use std::path::Path;

/// Output of a delivery-link resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// SHA of the delivering commit on main, if one could be resolved.
    pub sha: Option<String>,
    /// True when the task file's hint disagrees with the resolution
    /// (hint pointed at a different SHA, or at nothing verifiable).
    /// Callers that intend to persist corrections can check this to
    /// decide whether to rewrite the task file.
    pub hint_stale: bool,
}

/// Resolve the delivering commit for `task`. Consults the hint first,
/// falls back to a tag scan on main. Returns an empty result if the
/// git state can't be queried (e.g., `repo_root` isn't a git repo).
pub fn resolve(repo_root: &Path, task: &Task) -> Delivery {
    let Ok(main_branch) = git::git_current_branch(repo_root) else {
        return Delivery { sha: None, hint_stale: false };
    };
    let tag = format!("[{}]", task.id);
    if let Some(hint) = &task.delivered_in {
        if git::git_is_ancestor(repo_root, hint, &main_branch)
            && git::git_commit_subject(repo_root, hint)
                .map(|s| s.contains(&tag))
                .unwrap_or(false)
        {
            return Delivery {
                sha: Some(hint.clone()),
                hint_stale: false,
            };
        }
        // Hint doesn't verify — fall through to the tag scan. Mark
        // stale only if the tag scan finds a *different* answer.
        let resolved = git::git_log_find_subject(repo_root, &main_branch, &tag);
        let stale = match (&resolved, hint) {
            (Some(sha), h) => sha != h,
            (None, _) => true,
        };
        return Delivery {
            sha: resolved,
            hint_stale: stale,
        };
    }
    Delivery {
        sha: git::git_log_find_subject(repo_root, &main_branch, &tag),
        hint_stale: false,
    }
}

/// Human-friendly `"<short> <subject>"` for display in `bl show`.
pub fn describe(repo_root: &Path, sha: &str) -> String {
    let short = git::git_short_sha(repo_root, sha).unwrap_or_else(|| sha.to_string());
    match git::git_commit_subject(repo_root, sha) {
        Some(subj) if !subj.is_empty() => format!("{} {}", short, subj),
        _ => short,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{NewTaskOpts, Task};
    use tempfile::TempDir;

    fn empty_task() -> Task {
        Task::new(
            NewTaskOpts {
                title: "t".into(),
                ..Default::default()
            },
            "bl-abcd".into(),
        )
    }

    #[test]
    fn resolve_returns_empty_when_not_a_git_repo() {
        let dir = TempDir::new().unwrap();
        let d = resolve(dir.path(), &empty_task());
        assert!(d.sha.is_none());
        assert!(!d.hint_stale);
    }

    #[test]
    fn describe_falls_back_to_short_sha_when_no_subject() {
        // A tempdir isn't a git repo, so both subject and short-sha
        // lookups return None — describe falls back to the raw sha.
        let dir = TempDir::new().unwrap();
        let out = describe(dir.path(), "deadbeef");
        assert_eq!(out, "deadbeef");
    }
}
