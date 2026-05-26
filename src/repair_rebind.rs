//! `bl repair --rebind-path` — move the per-clone state subtree from
//! the old `<nested-clone-path>` to the new one after a clone moves
//! per SPEC-clone-layout §8 + §14.14 + Phase 3 (bl-05e5).
//!
//! The action verb counterpart to [`crate::doctor_moved`]'s read-only
//! finding. Doctor names every orphaned subtree; this function moves
//! each one with a single atomic `rename` (no copy fallback).
//! `rename` on Linux is atomic within a filesystem; if `~/.local/state`
//! and the user's per-clone state cross filesystems the call returns
//! an error rather than silently degrading to a copy — same-FS is the
//! only supported configuration for XDG state in this SPEC.
//!
//! Refusal contract (SPEC §14.14): if the destination
//! `<new-nested>/` already exists with content, the rebind aborts.
//! A user staring at two real per-clone subtrees needs to decide
//! manually which to keep; we won't merge them silently.

use crate::clone_breadcrumb;
use crate::doctor_moved::{find_orphans, OrphanClone};
use crate::encoding::nested_clone_path;
use crate::error::{BallError, Result};
use crate::xdg_paths::{PerClonePaths, XdgBases};
use std::fs;
use std::path::{Path, PathBuf};

/// One sibling subtree under the four per-clone roots
/// (`worktrees/`, `claims/`, `locks/`, `plugins-auth/`). Bundling them
/// keeps the move loop's intent visible — every sibling moves
/// together, none on their own.
struct Move {
    from: PathBuf,
    to: PathBuf,
}

/// Outcome of one orphan's rebind: every subtree that actually moved
/// (some siblings may not exist for every orphan — e.g. `worktrees/`
/// only present when the clone held a claim at move time). Returned
/// so the CLI can print one line per moved subtree.
#[derive(Debug)]
pub struct RebindReport {
    pub nested_from: PathBuf,
    pub nested_to: PathBuf,
    pub moved: Vec<PathBuf>,
}

/// Run the rebind from the current working directory's clone. Resolves
/// XDG bases from the environment, finds the orphans
/// [`crate::doctor_moved`] would surface, and rebinds each one onto
/// the current `<nested-clone-path>`. Returns one [`RebindReport`]
/// per orphan rebound.
pub fn run(current_clone_root: &Path) -> Result<Vec<RebindReport>> {
    let bases = XdgBases::from_env()
        .ok_or_else(|| BallError::Other("HOME is not set; cannot resolve XDG paths".into()))?;
    run_with(&bases, current_clone_root)
}

/// Pure version of [`run`] — takes resolved bases. The test seam
/// callers exercise: env mutation is process-global and races
/// parallel tests (bl-bfa8/bl-ad4b), so the test code passes bases
/// in directly and the CLI thin wrapper handles the env read.
pub fn run_with(bases: &XdgBases, current_clone_root: &Path) -> Result<Vec<RebindReport>> {
    let orphans = find_orphans(bases, current_clone_root);
    if orphans.is_empty() {
        return Ok(Vec::new());
    }
    let nested_to = nested_clone_path(current_clone_root);
    let mut reports = Vec::new();
    for o in orphans {
        reports.push(rebind_one(bases, &o, &nested_to, current_clone_root)?);
    }
    Ok(reports)
}

/// Rebind one orphan. The four per-clone roots are siblings; each
/// pre-existing source subtree moves to the matching destination.
/// After all moves succeed, the breadcrumb at the destination is
/// rewritten so the recorded path matches the new clone path.
fn rebind_one(
    bases: &XdgBases,
    orphan: &OrphanClone,
    nested_to: &Path,
    current_clone_root: &Path,
) -> Result<RebindReport> {
    let from = PerClonePaths::new(bases, &orphan.nested);
    let to = PerClonePaths::new(bases, nested_to);
    let pending = [
        Move { from: from.worktrees.clone(), to: to.worktrees.clone() },
        Move { from: from.claims.clone(), to: to.claims.clone() },
        Move { from: from.locks.clone(), to: to.locks.clone() },
        Move { from: from.plugins_auth.clone(), to: to.plugins_auth.clone() },
    ];

    // Refusal precheck: destination siblings must not already exist
    // with content. An empty destination dir is fine (commonly created
    // by the next bl invocation at the new nested path).
    for mv in &pending {
        if mv.from.exists() && has_content(&mv.to) {
            return Err(BallError::Other(format!(
                "refusing rebind: {} already exists with content; \
                 resolve the conflict by hand before re-running",
                mv.to.display()
            )));
        }
    }

    let mut moved = Vec::new();
    for mv in &pending {
        if !mv.from.exists() {
            continue;
        }
        // Empty placeholder at dest: remove so rename succeeds.
        if mv.to.exists() {
            fs::remove_dir(&mv.to).ok();
        }
        if let Some(parent) = mv.to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&mv.from, &mv.to).map_err(|e| {
            BallError::Other(format!(
                "rename {} → {} failed: {e}",
                mv.from.display(),
                mv.to.display()
            ))
        })?;
        moved.push(mv.to.clone());
    }

    // Rewrite the breadcrumb at the new location so a future doctor
    // walk sees the clone as resident, not moved-again.
    clone_breadcrumb::write_at(&to.claims, current_clone_root)?;

    Ok(RebindReport {
        nested_from: orphan.nested.clone(),
        nested_to: nested_to.to_path_buf(),
        moved,
    })
}

/// True when `dir` exists and contains any entry. A bare directory
/// (no entries) is treated as absent — bl materializes empty
/// per-clone dirs on routine discover (SPEC §7 step 7), so an empty
/// `worktrees/<new-nested>/` is not a content conflict.
fn has_content(dir: &Path) -> bool {
    fs::read_dir(dir)
        .is_ok_and(|mut entries| entries.next().is_some())
}

#[cfg(test)]
#[path = "repair_rebind_tests.rs"]
mod tests;
