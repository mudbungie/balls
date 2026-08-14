//! The speculator pass (bl-d0c2, design
//! docs/design/bl-24e7-speculative-merge-queue.md): one idempotent sweep of
//! the merge queue that pre-pays gates so closes land as cache hits.
//!
//! One pass, in order: SWEEP unsealed queue entries (their branch moved — a
//! landed close deleted it, or a fix is coming and will re-tag; either way the
//! stale tag holds no position), then walk the SEALED entries head-first, then
//! ADOPT every quiet `work/<id>` tip not already sealed
//! ([`crate::speculate_queue::adopt`], bl-b761) — the paved path by which an
//! agent that never learns the queue exists still rides it. The walk chains
//! candidates with [`crate::speculate_candidate`], consulting the
//! verdict cache before ever building. Strict head-first order is the whole
//! scheduling theory: a candidate is only built when every shallower prefix
//! already holds a PASS, so the depth-risk the design worried about — building
//! on a predecessor that then evicts — is zero at build time, evictions being
//! gate failures and gate failures being known. What remains of the eagerness
//! knob is `builds` (how many gates one pass may spend — the caller's
//! watts-vs-wall-time declaration) and the chain stops: a CONFLICT or a FAIL
//! verdict ends the buildable prefix (deeper candidates contain the same
//! problem), and the queue's end ends the pass.
//!
//! The gate subprocess runs under `nice` in a detached build worktree,
//! removed before the pass reports — a close-time gate on cache miss runs
//! unniced by construction, so the real merge path always preempts. On a pass
//! the gate's own hook records the verdict; [`run`] records it again
//! (idempotent, same key) so a stub gate under test and the real gate behave
//! identically, and records FAIL itself, which the aborting hook never can.

use std::io;
use std::path::Path;
use std::process::Command;

use crate::speculate;
use crate::speculate_candidate::{self, Merge};
use crate::speculate_queue;

/// What one pass did, one line per event — the plain-text contract agents and
/// tests read back.
pub type Report = Vec<String>;

/// One speculator pass over the queue at `repo`. `onto` names the landing
/// branch the chain grows from; `gate` is the command run inside each
/// candidate (the real caller passes `scripts/pre-commit`); `builds` caps how
/// many gates this pass may spend.
pub fn run(
    repo: &Path,
    scratch: &Path,
    territory: &Path,
    toolchain: &str,
    onto: &str,
    gate: &str,
    builds: usize,
) -> io::Result<Report> {
    let mut report = Vec::new();
    let mut base = base_commit(repo, onto)?;
    // Keyed by the gate as THIS checkout sees it — deliberately. The verdict a
    // close will look up is keyed by the closer's gate; a candidate that edits
    // the gate files changes the fingerprint for everyone only once it lands.
    let gate_fp = speculate::gate_fingerprint(repo, scratch, toolchain)?;
    let mut spent = 0;
    for entry in speculate_queue::queue(repo)? {
        if !entry.sealed {
            speculate_queue::dequeue(repo, &entry.id)?;
            report.push(format!("swept {} (unsealed)", entry.id));
            continue;
        }
        let tree = match speculate_candidate::merge_tree(repo, &base, &entry.tip)? {
            Merge::Tree(tree) => tree,
            Merge::Conflict => {
                report.push(format!("conflict {} — fold-at-close, chain stops", entry.id));
                break;
            }
        };
        let candidate = speculate_candidate::commit_tree(repo, &tree, &[&base, &entry.tip])?;
        match speculate::read(territory, &tree, &gate_fp)? {
            Some(v) if v.pass => report.push(format!("hit {} {tree}", entry.id)),
            Some(_) => {
                report.push(format!("fail {} {tree} — chain stops", entry.id));
                break;
            }
            None if spent >= builds => {
                report.push(format!("deferred {} (builds spent)", entry.id));
                break;
            }
            None => {
                spent += 1;
                let pass = build(repo, scratch, &candidate, gate)?;
                let verdict = speculate::Verdict { pass, builder: "bl-speculate".to_string() };
                speculate::write(territory, &tree, &gate_fp, &verdict)?;
                if pass {
                    report.push(format!("built {} {tree} pass", entry.id));
                } else {
                    report.push(format!("built {} {tree} FAIL — chain stops", entry.id));
                    break;
                }
            }
        }
        base = candidate;
    }
    // ADOPT, last (bl-b761): seal every quiet `work/<id>` tip not already
    // sealed, so agents that only ever commit and close still ride the queue.
    // Pass-END placement is the debounce — a fresh seal must survive one full
    // inter-pass interval before the walk above will build it, so quiescence
    // is measured in passes and no clock (least of all a smeared commit
    // date) is consulted. Sweep + adopt across one pass IS requeue-at-bottom
    // for a moved tip.
    for (id, tip) in speculate_queue::adopt(repo, None)? {
        report.push(format!("adopted {id} {tip}"));
    }
    Ok(report)
}

/// The tip of the landing branch — where the candidate chain is rooted.
fn base_commit(repo: &Path, onto: &str) -> io::Result<String> {
    let mut cmd = crate::safegit::at(repo);
    cmd.args(["rev-parse", "--verify"]).arg(format!("refs/heads/{onto}"));
    let out = cmd.output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(io::Error::other(format!(
            "git rev-parse {onto}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Materialize the candidate, run the gate under `nice` inside it, tear the
/// worktree down whatever the outcome. Only the gate's exit code speaks.
fn build(repo: &Path, scratch: &Path, candidate: &str, gate: &str) -> io::Result<bool> {
    std::fs::create_dir_all(scratch)?;
    let dir = scratch.join(format!("build-{candidate}"));
    speculate_candidate::build_dir(repo, candidate, &dir)?;
    let status = Command::new("nice").arg("-n19").arg(gate).current_dir(&dir).status();
    speculate_candidate::remove_build_dir(repo, &dir)?;
    Ok(status?.success())
}

#[cfg(test)]
#[path = "speculate_run_tests.rs"]
mod speculate_run_tests;
