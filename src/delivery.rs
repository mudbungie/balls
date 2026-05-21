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
/// falls back to a tag scan on `main_branch` — the integration branch
/// the caller resolved through `Config::integration_branch` (the
/// single `target_branch` seam), so this stays a pure git query with
/// no config knowledge of its own. Returns an empty result if the git
/// state can't be queried (e.g., `repo_root` isn't a git repo, or the
/// branch doesn't exist).
pub fn resolve(repo_root: &Path, main_branch: &str, task: &Task) -> Delivery {
    let tag = format!("[{}]", task.id);
    if let Some(hint) = &task.delivered_in {
        if git::git_is_ancestor(repo_root, hint, main_branch)
            && git::git_commit_subject(repo_root, hint)
                .is_some_and(|s| s.contains(&tag))
        {
            return Delivery {
                sha: Some(hint.clone()),
                hint_stale: false,
            };
        }
        // Hint doesn't verify — fall through to the tag scan. Mark
        // stale only if the tag scan finds a *different* answer.
        let resolved = git::git_log_find_subject(repo_root, main_branch, &tag);
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
        sha: git::git_log_find_subject(repo_root, main_branch, &tag),
        hint_stale: false,
    }
}

/// Populate `task.delivered_in` at close time when it is still null
/// (SPEC §6; bl-87ea). Deferred-mode `bl review` never lands a local
/// squash, so it never writes the hint — by the time `bl close` runs
/// after the forge merges, the field is still null. This caches the
/// `[id]`-tagged merge commit into the task file the close commit
/// archives, so a later `bl show` resolves via the fast hint path.
///
/// `manual` (`bl close --delivered <sha>`) wins unconditionally and
/// skips the scan — the operator's explicit override for the case
/// where the forge produced a rebase-merge with several commits and
/// they want to point at a specific one. Otherwise this is a no-op
/// when the hint is already set (local-squash mode wrote it in
/// `review`, so that path stays byte-identical). When the hint is
/// null we reuse `resolve` — the same tag-scan machinery, so this is
/// a strict generalization, not a new mechanism — against the task's
/// effective `target_branch`. A miss warns and leaves the hint null:
/// the `[id]` tag in the merge subject is still ground truth, and the
/// half-push detector (which scans subjects, not this hint) is
/// unaffected.
///
/// Returns `true` iff `delivered_in` was set, so the close path knows
/// to persist the task to the state branch before archiving it (the
/// no-op local-squash path returns `false` and stays byte-identical).
pub fn populate_on_close(
    repo_root: &Path,
    target_branch: &str,
    task: &mut Task,
    manual: Option<String>,
) -> bool {
    // bl-7523: whenever we *set* `delivered_in` we also tag the
    // local repo as the delivery's source, so a reader on a hub (or
    // a sibling client) can resolve the sha even when no code clone
    // is locally checked out. The already-set path stays a no-op —
    // the local-squash review path already wrote both fields, and
    // pre-bl-7523 tasks without `delivered_repo` are read as "the
    // locally-checked-out repo" (no retrofit).
    let new_sha = if let Some(sha) = manual {
        Some(sha)
    } else if task.delivered_in.is_some() {
        return false;
    } else if let Some(sha) = resolve(repo_root, target_branch, task).sha {
        Some(sha)
    } else {
        eprintln!(
            "warning: no [{id}] commit reachable on {target_branch}; closing \
             without delivered_in (the [{id}] tag in the merge subject stays \
             ground truth)",
            id = task.id,
        );
        None
    };
    let Some(sha) = new_sha else { return false };
    task.delivered_in = Some(sha);
    task.delivered_repo = Some(crate::repo_url::current(repo_root));
    true
}

/// Human-friendly `"<short> <subject>"` for display in `bl show`.
pub fn describe(repo_root: &Path, sha: &str) -> String {
    let short = git::git_short_sha(repo_root, sha).unwrap_or_else(|| sha.to_string());
    match git::git_commit_subject(repo_root, sha) {
        Some(subj) if !subj.is_empty() => format!("{short} {subj}"),
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
        // Not a git repo: every git query fails, so resolve yields an
        // empty, non-stale result regardless of the branch passed.
        let dir = TempDir::new().unwrap();
        let d = resolve(dir.path(), "main", &empty_task());
        assert!(d.sha.is_none());
        assert!(!d.hint_stale);
    }

    #[test]
    fn populate_on_close_manual_override_wins_unconditionally() {
        // `bl close --delivered <sha>` skips the scan and sets the
        // hint even when one is already present (forge rebase-merge).
        // bl-7523: the manual sha is by definition local-resolvable,
        // so `delivered_repo` is tagged with the current repo.
        let dir = TempDir::new().unwrap();
        let mut t = empty_task();
        t.delivered_in = Some("oldsha".into());
        let changed = populate_on_close(dir.path(), "main", &mut t, Some("forced".into()));
        assert!(changed);
        assert_eq!(t.delivered_in.as_deref(), Some("forced"));
        assert_eq!(
            t.delivered_repo.as_deref(),
            Some(crate::repo_url::current(dir.path()).as_str())
        );
    }

    #[test]
    fn populate_on_close_is_noop_when_hint_already_set() {
        // Local-squash mode wrote the hint in `review`; close must
        // not touch it (no scan, byte-identical archived task). The
        // bl-7523 provenance the review path already wrote stays put.
        let dir = TempDir::new().unwrap();
        let mut t = empty_task();
        t.delivered_in = Some("fromreview".into());
        t.delivered_repo = Some("git@h:from-review.git".into());
        let changed = populate_on_close(dir.path(), "main", &mut t, None);
        assert!(!changed);
        assert_eq!(t.delivered_in.as_deref(), Some("fromreview"));
        assert_eq!(t.delivered_repo.as_deref(), Some("git@h:from-review.git"));
    }

    #[test]
    fn populate_on_close_scan_miss_leaves_hint_null() {
        // Null hint, no `[id]` commit reachable (not a git repo, so
        // the tag scan finds nothing): warn and proceed with null —
        // no sha, no provenance.
        let dir = TempDir::new().unwrap();
        let mut t = empty_task();
        let changed = populate_on_close(dir.path(), "main", &mut t, None);
        assert!(!changed);
        assert!(t.delivered_in.is_none());
        assert!(t.delivered_repo.is_none());
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
