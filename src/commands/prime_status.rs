//! Per-claim status indicators for `bl prime`. Two signals catch the
//! parallel-agent collision class that the lernie postmortem
//! (2026-04-22) walked into:
//!
//! - `main_ahead`: how many commits main has gained since this claim's
//!   work branch was forked. Surfaces the "main moved during my work"
//!   condition before it becomes a noisy diagnostic at commit time.
//! - `overlap_files`: files that appear in BOTH main's diff since the
//!   fork point AND the work branch's diff. A non-empty intersection
//!   is a high-probability merge conflict — or worse, a sibling task
//!   landing changes that contradict yours.

use balls::git;
use balls::store::Store;
use balls::task::Task;
use std::collections::HashSet;
use std::path::Path;

pub struct ClaimStatus {
    pub main_ahead: usize,
    pub overlap_files: Vec<String>,
}

/// Compute both signals for one claimed task. Silent fallback on every
/// failure mode (no main branch, no work branch, git error) — the
/// indicator is advisory, never fatal.
pub fn for_task(store: &Store, task: &Task, main_branch: Option<&str>) -> ClaimStatus {
    let Some((main, branch)) = main_branch.zip(task.branch.as_deref()) else {
        return ClaimStatus { main_ahead: 0, overlap_files: Vec::new() };
    };
    ClaimStatus {
        main_ahead: count_ahead(&store.root, branch, main),
        overlap_files: overlap(&store.root, branch, main),
    }
}

/// `git rev-list --count base..head` counts commits reachable from
/// head but not base — exactly the "main moved this far past your
/// fork point" metric.
fn count_ahead(dir: &Path, base: &str, head: &str) -> usize {
    let range = format!("{base}..{head}");
    git::clean_git_command(dir)
        .args(["rev-list", "--count", &range])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}

/// Files touched on BOTH sides of the fork. Uses `A...B` triple-dot
/// ranges so each call is "from merge-base to tip" — symmetric.
fn overlap(dir: &Path, work: &str, main: &str) -> Vec<String> {
    let main_files: HashSet<String> = changed_files(dir, work, main).into_iter().collect();
    if main_files.is_empty() {
        return Vec::new();
    }
    let mut overlap: Vec<String> = changed_files(dir, main, work)
        .into_iter()
        .filter(|f| main_files.contains(f))
        .collect();
    overlap.sort();
    overlap
}

fn changed_files(dir: &Path, base: &str, head: &str) -> Vec<String> {
    let range = format!("{base}...{head}");
    git::clean_git_command(dir)
        .args(["diff", "--name-only", &range])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}
