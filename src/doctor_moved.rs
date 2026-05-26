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
//! 2. Filter to breadcrumbs whose `hostname` matches this host (the
//!    SPEC §12 NFS guard — subtrees owned by another host on a shared
//!    `$HOME` are not "moved from this host's perspective").
//! 3. A subtree is moved-from-here when the breadcrumb sits at a
//!    different nested path than the current clone, records a path
//!    other than the current clone's path, and that recorded path no
//!    longer exists on disk. The last check disambiguates a real move
//!    from a second active clone living elsewhere on the same host —
//!    if `/home/u/old` still exists as a directory, the breadcrumb is
//!    owned by that live clone, not orphaned by a move.

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
    /// Clone path the breadcrumb recorded — the path the user `mv`d
    /// away from. Always present: a breadcrumb that won't parse can't
    /// be classified as moved-from-here, so the walk skips it.
    pub recorded_path: String,
    /// Task IDs the orphaned claims/ holds. Drawn from the file names
    /// of every regular file under `claims_dir` *except* the breadcrumb.
    pub orphan_task_ids: Vec<String>,
}

/// Run the moved-clone walk. Returns orphans whose breadcrumb records
/// a clone path that differs from `current_clone_root` and no longer
/// exists on disk — i.e. the user `mv`d the clone away from there.
/// Same-host filtering happens here so the doctor caller doesn't have
/// to know about hostnames.
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
        // Decision needs a readable breadcrumb. A corrupt one is a
        // different failure mode — we don't have a recorded path to
        // classify against, so we can't call it moved-from-here.
        let Some(bc) = clone_breadcrumb::read_at(claims_dir) else { return };
        // Cross-host guard: SPEC §12 NFS scoping.
        if bc.hostname != host {
            return;
        }
        // Same-nested as current clone → our own tree, not an orphan.
        if nested == current_nested {
            return;
        }
        // Breadcrumb pointing at this very clone from a different
        // nested path is defensive-only (writer shouldn't produce it);
        // either way, the clone isn't moved-away-from-here.
        if bc.path == current_path_str {
            return;
        }
        // Disambiguate move from "another active clone on this host":
        // a still-present recorded path belongs to a live sibling, not
        // an orphan. A gone recorded path is a move (or delete, which
        // the rebind also fixes).
        if Path::new(&bc.path).exists() {
            return;
        }
        orphans.push(OrphanClone {
            claims_dir: claims_dir.to_path_buf(),
            nested: nested.to_path_buf(),
            recorded_path: bc.path,
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
            Finding::flag(
                format!(
                    "moved clone detected: per-clone state at {} records path {} ({})",
                    o.claims_dir.display(),
                    o.recorded_path,
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
