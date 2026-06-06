//! §16 delivery doctor — the CODE-worktree drift audit base balls cannot see.
//!
//! Base doctor ([`crate::doctor`]) reads only the §8 change scratch under
//! `clones/<enc>/changes/`; the deliverable worktree lives in THIS plugin's
//! own §1 territory at the derived [`crate::delivery::worktree_path`], a path
//! base never computes nor opens (it would have to read the §7 binding and the
//! project repo, which base does not do). So the plugin that OWNS the territory
//! contributes the check, wired into the `doctor` read op like any diffless
//! op's plugin chain (§16).
//!
//! The audit is a partition. One side is the balls the actor still CLAIMS —
//! the §3 occupancy field, read off `operating/tasks/` by the actor-scoped
//! [`crate::delivery_repo::claimed_ids`] (the SAME set `prime` re-materializes,
//! so only those can drift here). The other is the MATERIALIZED worktrees — the
//! `<id>/` children of this binding's territory ([`materialized_ids`]). A
//! claimed id with NO worktree is a failed `claim.post` or a hand-deletion (`bl
//! prime` idempotently re-materializes it); a worktree with NO live claim is
//! failed `close.post`/`drop.post` teardown — the human runs the named `git
//! worktree remove` (it may hold uncommitted work, §16). Findings print to the
//! `doctor` stream; reads have no return channel (§7), same as base doctor.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use crate::doctor::Finding;

/// Partition `claimed` against `materialized`, deriving each finding's worktree
/// path with `path_of`. The two `difference`s are the two drift classes; the
/// intersection (a claim with its worktree) is the healthy case and yields
/// nothing. Pure over its inputs — the filesystem reads ([`claimed_ids`] /
/// [`materialized_ids`]) and the path formula are injected, so the partition is
/// testable without a temp repo.
#[must_use]
pub fn audit(claimed: &BTreeSet<String>, materialized: &BTreeSet<String>, path_of: &dyn Fn(&str) -> String) -> Vec<Finding> {
    let mut findings = Vec::new();
    for id in claimed.difference(materialized) {
        findings.push(Finding {
            drift: format!("claimed ball {id} has no code worktree: {}", path_of(id)),
            fix: "bl prime (idempotently re-materializes the worktree)".into(),
        });
    }
    for id in materialized.difference(claimed) {
        let path = path_of(id);
        findings.push(Finding {
            drift: format!("orphan code worktree (no live claim): {path}"),
            fix: format!("git worktree remove {path} — may hold uncommitted work, inspect first"),
        });
    }
    findings
}

/// Render the audit as this plugin's slice of the `doctor` stream (§7, no
/// return channel) — delivery-owned, so it is NOT base doctor's `Report`
/// (which speaks of "core-owned" findings). Empty ⇒ one clean line; else a
/// header plus each finding's drift and fix.
#[must_use]
pub fn render(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "delivery: no code-worktree drift detected\n".into();
    }
    let mut out = format!("delivery: {} code-worktree finding(s)\n", findings.len());
    for f in findings {
        let _ = writeln!(out, "  - {}\n    fix: {}", f.drift, f.fix);
    }
    out
}

/// The ids of MATERIALIZED worktrees in this binding's `territory` — the other
/// half of the [`audit`] partition. Each `<id>/` subdir is one code worktree
/// (the dir name IS the id, §1/§11); a stray file is ignored, an absent
/// territory yields none.
pub fn materialized_ids(territory: &Path) -> io::Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    for entry in dir_entries(territory)? {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                ids.insert(name.to_string());
            }
        }
    }
    Ok(ids)
}

/// The immediate entries under `dir`, or empty when it is absent — the
/// best-effort directory read both gatherers share (§16: a read op surfaces
/// drift, never an I/O error of its own).
fn dir_entries(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    fs::read_dir(dir)?.collect()
}

#[cfg(test)]
#[path = "delivery_doctor_tests.rs"]
mod tests;
