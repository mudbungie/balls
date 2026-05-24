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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Delivery {
    /// SHA of the delivering commit on main, if one could be resolved.
    pub sha: Option<String>,
    /// True when the task file's hint disagrees with the resolution
    /// (hint pointed at a different SHA, or at nothing verifiable).
    /// Callers that intend to persist corrections can check this to
    /// decide whether to rewrite the task file.
    pub hint_stale: bool,
    /// Identifier of the code repo whose history yielded `sha` (bl-f37b).
    /// Always `Some` whenever `sha` is `Some`. Local hits report
    /// `repo_url::current(repo_root)` so tooling on a sibling clone or
    /// the tracker knows the resolution came from this clone; remote
    /// hits (cross-repo lookup via `delivered_repo`) report the task's
    /// `delivered_repo` value verbatim. Callers thread this back to
    /// the JSON contract as `delivered_in_resolved_repo`.
    pub resolved_repo: Option<String>,
}

/// Resolve the delivering commit for `task`. Consults the hint first,
/// falls back to a tag scan on `main_branch` — the integration branch
/// the caller resolved through `Config::integration_branch` (the
/// single `target_branch` seam), so this stays a pure git query with
/// no config knowledge of its own. Returns an empty result if the git
/// state can't be queried (e.g., `repo_root` isn't a git repo, or the
/// branch doesn't exist).
pub fn resolve(repo_root: &Path, main_branch: &str, task: &Task) -> Delivery {
    resolve_with(repo_root, main_branch, task, ResolveOpts::default())
}

/// Caller-side knobs for [`resolve_with`]. `remote` opts in to the
/// cross-repo fallback (bl-f37b): on a local miss with `delivered_repo`
/// set, fetch a balls-owned cache of that repo and re-run the tag scan.
/// Off by default so single-repo callers and the legacy `resolve`
/// surface stay byte-identical.
#[derive(Debug, Clone, Default)]
pub struct ResolveOpts {
    pub remote: bool,
}

/// Same contract as [`resolve`] with optional cross-repo fallback. The
/// local-only path is unchanged; the remote path engages only when the
/// local scan returns no sha *and* the task carries a `delivered_repo`
/// that names something we can fetch. A remote miss is soft — `sha`
/// stays `None`, `hint_stale` is set from the local scan, and the
/// command proceeds.
pub fn resolve_with(
    repo_root: &Path,
    main_branch: &str,
    task: &Task,
    opts: ResolveOpts,
) -> Delivery {
    let tag = format!("[{}]", task.id);
    let local = local_resolve(repo_root, main_branch, task, &tag);
    if local.sha.is_some() {
        return Delivery {
            resolved_repo: Some(crate::repo_url::current(repo_root)),
            ..local
        };
    }
    let Some(url) = task.delivered_repo.as_deref().filter(|_| opts.remote) else {
        return local;
    };
    match crate::delivery_remote::resolve(repo_root, url, task) {
        Some(sha) => Delivery {
            sha: Some(sha),
            hint_stale: local.hint_stale,
            resolved_repo: Some(url.to_string()),
        },
        None => local,
    }
}

fn local_resolve(repo_root: &Path, main_branch: &str, task: &Task, tag: &str) -> Delivery {
    if let Some(hint) = &task.delivered_in {
        if git::git_is_ancestor(repo_root, hint, main_branch)
            && git::git_commit_subject(repo_root, hint)
                .is_some_and(|s| s.contains(tag))
        {
            return Delivery {
                sha: Some(hint.clone()),
                hint_stale: false,
                resolved_repo: None,
            };
        }
        // Hint doesn't verify — fall through to the tag scan. Mark
        // stale only if the tag scan finds a *different* answer.
        let resolved = git::git_log_find_subject(repo_root, main_branch, tag);
        let stale = match (&resolved, hint) {
            (Some(sha), h) => sha != h,
            (None, _) => true,
        };
        return Delivery {
            sha: resolved,
            hint_stale: stale,
            resolved_repo: None,
        };
    }
    Delivery {
        sha: git::git_log_find_subject(repo_root, main_branch, tag),
        hint_stale: false,
        resolved_repo: None,
    }
}

/// Populate `task.delivered_in` at close time when it is still null
/// (SPEC §6; bl-87ea). Deferred-mode `bl review` never lands a local
/// squash, so it never writes the hint — by the time `bl close` runs
/// after the forge merges, the field is still null. This caches the
/// `[id]`-tagged merge commit into the task file the close commit
/// archives, so a later `bl show` resolves via the fast hint path.
///
/// `manual_sha` (`bl close --delivered <sha>`) wins unconditionally
/// and skips the scan — the operator's explicit override for the case
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
/// `manual_repo` (`bl close --delivered-repo <url>`, bl-733e) is the
/// parallel override for `delivered_repo`. The auto-tag default is
/// the repo the sha was *resolved through* (bl-6816): the current
/// clone's `origin` for a local hit or a hand-supplied sha, the
/// source URL for a `--resolve-remote` cross-repo hit. A bridge
/// clone running close from a sync hook on behalf of *another* repo
/// (README §Bridging) needs to declare the true source: this flag
/// does that. It always wins over the auto-tag and can be set with
/// or without `--delivered`. Setting it on a task with no sha
/// (manual or scanned) is allowed but odd — the operator opted in.
///
/// `resolve_remote` (`bl close --resolve-remote`, bl-e454) routes the
/// local-miss case through the cross-repo cache when the task carries
/// `delivered_repo`. It is the symmetric write-side counterpart to
/// `bl show --resolve-remote` (bl-f37b): on the tracker or a sibling
/// clone whose history doesn't carry the `[bl-xxxx]` squash, an
/// unattended `bl close` would otherwise archive the task with
/// `delivered_in: null` even though the sha is one fetch away.
/// Off by default — fetches from arbitrary forge URLs need operator
/// opt-in, same policy as the show path. Deferred-mode is set
/// implicitly upstream so a bridge running deferred close from a
/// sync hook gets resolution without a flag. A remote hit also
/// carries the source URL into `delivered_repo` (bl-6816): the
/// closing clone is not the delivering repo, so auto-tagging its
/// own `origin` would aim a later `--resolve-remote` read at a
/// tracker that holds no `[bl-xxxx]` tag — the flag would destroy
/// the provenance it exists to recover.
///
/// Returns `true` iff anything changed, so the close path knows to
/// persist the task to the state branch before archiving it (the
/// no-op local-squash path returns `false` and stays byte-identical).
pub fn populate_on_close(
    repo_root: &Path,
    target_branch: &str,
    task: &mut Task,
    manual_sha: Option<String>,
    manual_repo: Option<String>,
    resolve_remote: bool,
) -> bool {
    // bl-7523: whenever we *set* `delivered_in` we also tag the
    // delivery's source repo so a reader on the tracker (or a sibling
    // client) can resolve the sha. bl-6816: the source is the repo
    // the sha was *resolved through* — `repo_url::current` for a
    // local hit or a hand-supplied sha (the operator has it locally,
    // so bl-7523's same-clone assumption holds), the source URL for
    // a `--resolve-remote` cross-repo hit. bl-733e: `manual_repo`
    // overrides whatever we pick.
    let new_delivery = if let Some(sha) = manual_sha {
        Some((sha, crate::repo_url::current(repo_root)))
    } else if task.delivered_in.is_some() {
        None
    } else if let Delivery {
        sha: Some(sha),
        resolved_repo: Some(repo),
        ..
    } = resolve_with(
        repo_root,
        target_branch,
        task,
        ResolveOpts { remote: resolve_remote },
    ) {
        Some((sha, repo))
    } else {
        eprintln!(
            "warning: no [{id}] commit reachable on {target_branch}; closing \
             without delivered_in (the [{id}] tag in the merge subject stays \
             ground truth)",
            id = task.id,
        );
        None
    };
    let mut changed = false;
    if let Some((sha, repo)) = new_delivery {
        task.delivered_in = Some(sha);
        task.delivered_repo = Some(repo);
        changed = true;
    }
    if let Some(url) = manual_repo {
        // Operator override: always wins, even when no sha was
        // written this pass (e.g. correcting the source repo on an
        // already-set delivered_in).
        task.delivered_repo = Some(url);
        changed = true;
    }
    changed
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
#[path = "delivery_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "delivery_close_remote_tests.rs"]
mod close_remote_tests;
