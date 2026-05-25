//! Moved-clone detection for [`crate::doctor`] per SPEC-clone-layout
//! §8 + §14.14 + Phase 3 (bl-05e5).
//!
//! When a user `mv`s their clone, the per-clone state under
//! `~/.local/state/balls/{worktrees,claims,locks,plugins-auth}/<old>/`
//! is orphaned at the old nested path while the next `bl` invocation
//! materializes a fresh tree at the new nested path. SPEC §8 asks
//! doctor to surface that orphan with the recorded path, the new
//! path, the orphaned task IDs, and the exact `bl repair --rebind-path`
//! command that would fix it.
//!
//! Mechanism:
//!
//! 1. Walk `~/.local/state/balls/claims/` recursively for every file
//!    named `clone-path.json` (the breadcrumb dropped by
//!    [`crate::clone_breadcrumb`] at materialization time).
//! 2. For each breadcrumb whose `hostname` matches this host, check
//!    whether the recorded `path` resolves to the directory the
//!    breadcrumb itself sits under. A match means the clone is still
//!    there; a mismatch means the clone moved (or the breadcrumb is
//!    stale).
//! 3. Compare each orphan's recorded path against the current clone's
//!    absolute path — if equal, surface a "rebind to <new-nested>"
//!    finding with the orphan's task IDs and the rebind command.
//!
//! Cross-host scoping: the hostname filter is the SPEC §12 NFS guard.
//! A per-clone subtree owned by another host on a shared `$HOME` is
//! not "moved from this host's perspective" — it is owned by that
//! host. Doctor reports nothing for it.

use crate::clone_breadcrumb::{self, BREADCRUMB_FILE};
use crate::doctor::Finding;
use crate::encoding::nested_clone_path;
use crate::xdg_paths::XdgBases;
use std::fs;
use std::path::{Path, PathBuf};

/// One orphaned per-clone subtree the doctor walk found. Public for
/// tests; the doctor wraps these into `Finding`s for the CLI surface.
#[derive(Debug, Clone)]
pub struct OrphanClone {
    /// Absolute path of `claims/<nested>/` on disk (where the
    /// breadcrumb sits).
    pub claims_dir: PathBuf,
    /// `<nested-clone-path>` of the orphaned subtree, relative to
    /// `~/.local/state/balls/claims/`.
    pub nested: PathBuf,
    /// Clone path the breadcrumb recorded — `None` if the breadcrumb
    /// is missing/corrupt (a different kind of orphan; we still report
    /// it so the user knows the subtree exists).
    pub recorded_path: Option<String>,
    /// Task IDs the orphaned claims/ holds. Drawn from the file names
    /// of every regular file under `claims_dir` *except* the breadcrumb.
    pub orphan_task_ids: Vec<String>,
}

/// Run the moved-clone walk. Returns the orphans whose recorded path
/// equals `current_clone_root`'s absolute path — i.e. the clone moved
/// and these are its old subtrees. Same-host filtering happens here so
/// the doctor caller doesn't have to know about hostnames.
///
/// `bases` is the resolved XDG bases (test override surface);
/// `current_clone_root` is the clone's absolute path as `Store::root`
/// reports it.
#[must_use]
pub fn find_orphans(bases: &XdgBases, current_clone_root: &Path) -> Vec<OrphanClone> {
    let claims_root = bases.state_root().join("claims");
    if !claims_root.exists() {
        return Vec::new();
    }
    let current_path_str = current_clone_root.to_string_lossy().into_owned();
    let current_nested = nested_clone_path(current_clone_root);
    let host = clone_breadcrumb::hostname();
    let mut orphans = Vec::new();
    walk_for_breadcrumbs(&claims_root, &claims_root, &mut |claims_dir, nested| {
        let bc = clone_breadcrumb::read_at(claims_dir);
        // Cross-host guard: breadcrumb hostname must match this host.
        if bc.as_ref().is_some_and(|b| b.hostname != host) {
            return;
        }
        // Same-nested as current clone → not an orphan, just our tree.
        if nested == current_nested {
            return;
        }
        // Only surface orphans recorded as the *same clone* we're
        // running against — i.e. the clone moved. A breadcrumb for a
        // different clone on the same host is owned by that clone;
        // doctor running here has nothing to say.
        let recorded_path = bc.as_ref().map(|b| b.path.clone());
        if recorded_path.as_deref() != Some(&*current_path_str) {
            return;
        }
        orphans.push(OrphanClone {
            claims_dir: claims_dir.to_path_buf(),
            nested: nested.to_path_buf(),
            recorded_path,
            orphan_task_ids: collect_task_ids(claims_dir),
        });
    });
    orphans
}

/// Convert the orphan list into doctor findings. One `Finding` per
/// orphan: the problem text names the old path and orphan task IDs,
/// the hint quotes the exact `bl repair --rebind-path` command.
#[must_use]
pub fn to_findings(orphans: &[OrphanClone], current_clone_root: &Path) -> Vec<Finding> {
    orphans
        .iter()
        .map(|o| {
            let id_summary = if o.orphan_task_ids.is_empty() {
                "no claimed tasks".to_string()
            } else {
                format!("claimed tasks: {}", o.orphan_task_ids.join(", "))
            };
            let recorded = o.recorded_path.as_deref().unwrap_or("<missing breadcrumb>");
            Finding::flag(
                format!(
                    "moved clone detected: per-clone state at {} records path {} ({})",
                    o.claims_dir.display(),
                    recorded,
                    id_summary,
                ),
                format!(
                    "run `bl repair --rebind-path` from {} to relocate the orphaned state",
                    current_clone_root.display(),
                ),
            )
        })
        .collect()
}

/// Recursively walk `dir` looking for `clone-path.json` files. For
/// each one found, invoke `f(claims_dir, nested)` where
/// `claims_dir` is the directory holding the breadcrumb and
/// `nested` is the path of `claims_dir` relative to `claims_root`.
/// A breadcrumb-bearing directory is *not* recursed into — per-clone
/// trees don't nest.
fn walk_for_breadcrumbs(
    dir: &Path,
    claims_root: &Path,
    f: &mut dyn FnMut(&Path, &Path),
) {
    if dir.join(BREADCRUMB_FILE).exists() {
        let nested = dir.strip_prefix(claims_root).unwrap_or(dir).to_path_buf();
        f(dir, &nested);
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_for_breadcrumbs(&p, claims_root, f);
        }
    }
}

/// Read every regular file in `claims_dir` whose name starts with
/// `bl-` and return the file names as task IDs. The breadcrumb file
/// is filtered out by the `bl-` prefix.
fn collect_task_ids(claims_dir: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    let Ok(entries) = fs::read_dir(claims_dir) else { return ids };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("bl-") && e.path().is_file() {
            ids.push(name);
        }
    }
    ids.sort();
    ids
}

#[cfg(test)]
#[path = "doctor_moved_tests.rs"]
mod tests;
