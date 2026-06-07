//! §16 doctor — drift diagnosis as an ordinary read op.
//!
//! `bl doctor` is a diffless op (§6 — "reads are not special-cased"): it
//! authors no ball-file diff and NEVER mutates. Its whole job is to REPORT
//! drift and, per finding, name the existing verb or deliberate act that fixes
//! it — `bl repair` is retired (§16, "No repair verb"). Findings ARE the
//! diagnostics: reads have no return channel (§7), and for doctor that is the
//! feature.
//!
//! This module is BASE doctor: it audits only core-owned structure — what base
//! balls itself creates and can see without opening the project repo or reading
//! a plugin's config (§0). Each plugin contributes checks for ITS OWN §1
//! territory by wiring a binary into the `doctor` hook dirs, exactly as every
//! diffless op's behavior is the union of its run-parts chain; base balls
//! cannot audit plugin territory, so it DELEGATES. The base checks here:
//!
//! - stale CHANGE worktrees under `clones/<enc>/changes/<uuid>/` — crash debris
//!   from an op whose teardown (§8) never completed.
//! - the `config/` landing resolves to a real checkout (§1/§2).
//! - the registry's LOCAL `bin/` dangle — a committed `NN-<name>` whose
//!   machine-local `bin/<name>` is missing (§6, the one artifact only doctor
//!   can surface).
//! - protocol-version drift — a wired plugin whose `protocol` no longer
//!   includes balls' current version.
//! - circular blockers (§10) — a cycle in the `blockers` edges across `tasks/`.
//!
//! - the §4 [`EffectiveConfig`] resolves (the config subsystem): the landing's
//!   `config/balls.toml` parses, and the resolved `tasks_branch` is usable — an
//!   empty `tasks_branch` has no store to root on, so doctor surfaces it before
//!   an op fails.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::EffectiveConfig;
use crate::layout::CloneDir;
use crate::message::PROTOCOL;
use crate::plugin::Protocol;
use crate::registry::Registry;
use crate::task::Task;

/// One drift finding: what diverged, and the existing verb or act that fixes it
/// idempotently (§16 — never a `repair` verb).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub drift: String,
    pub fix: String,
}

/// The base audit's result — the union of core-owned findings. Empty means base
/// balls sees no drift it owns (a plugin's `doctor` hook may still find its
/// own, §16). [`Report`] renders straight to the diagnostic stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<Finding>,
}

/// Audit the core-owned structure of one clone bundle. `user_config` is the XDG
/// `config.toml` path (the §4 outermost-but-one layer); `probe` re-queries a
/// plugin binary's `protocol` (the edge passes [`crate::plugin::describe`];
/// tests pass a fake) — both injected so the audit stays a pure, testable read.
pub fn audit(
    clone: &CloneDir,
    user_config: &Path,
    probe: &dyn Fn(&Path) -> io::Result<Protocol>,
) -> io::Result<Report> {
    let mut findings = Vec::new();
    stale_changes(clone, &mut findings)?;
    let (landing, store) = (clone.landing(), clone.store());
    if landing.is_dir() {
        registry_drift(&landing, probe, &mut findings)?;
        circular_blockers(&store, &mut findings)?;
        config_resolution(&landing, user_config, &mut findings);
    } else {
        findings.push(Finding::landing_unresolved(&landing));
    }
    Ok(Report { findings })
}

/// Resolve the §4 config from the LANDING (then the XDG `user_config`) and check
/// it can drive core: a parse/projection failure, or a resolved `tasks_branch`
/// that would break the task store, is drift a `config/balls.toml` edit + `bl
/// prime` clears. Reading config is a local act (§4 — no fetch, no trail).
fn config_resolution(landing: &Path, user_config: &Path, findings: &mut Vec<Finding>) {
    match EffectiveConfig::resolve(landing, user_config) {
        Err(e) => findings.push(Finding::config_unresolved(&e.to_string())),
        Ok(cfg) => {
            if let Some(reason) = config_defect(&cfg) {
                findings.push(Finding::config_unusable(reason));
            }
        }
    }
}

/// Why a resolved §4 config cannot drive core, or `None` if it is usable. An
/// empty `tasks_branch` has no store branch to root on. (The id scheme is fixed,
/// not config — [`crate::id::IdScheme::default`] is always valid, so there is
/// nothing to validate there.)
fn config_defect(cfg: &EffectiveConfig) -> Option<&'static str> {
    cfg.tasks_branch
        .is_empty()
        .then_some("tasks_branch is empty — no store branch to root on")
}

/// Any leftover `changes/<uuid>/` is crash debris — an op whose teardown (§8)
/// never ran. Doctor names the precise `git worktree remove` and the human,
/// who may want the uncommitted work, runs it (§16 — never automated).
fn stale_changes(clone: &CloneDir, findings: &mut Vec<Finding>) -> io::Result<()> {
    for path in entries(&clone.root().join("changes"))? {
        findings.push(Finding::stale_change(&path));
    }
    Ok(())
}

/// A wired plugin whose local `bin/<name>` is missing is a dangle; one whose
/// `protocol` no longer speaks balls' version (or won't self-describe) is drift
/// — both fixed by `bl install` re-resolving and re-validating the binary (§6).
fn registry_drift(
    operating: &Path,
    probe: &dyn Fn(&Path) -> io::Result<Protocol>,
    findings: &mut Vec<Finding>,
) -> io::Result<()> {
    for w in Registry::at(operating).wired()? {
        match &w.plugin.bin {
            None => findings.push(Finding::dangling_plugin(&w.plugin.name)),
            Some(bin) => {
                if !probe(bin).is_ok_and(|p| p.speaks(PROTOCOL)) {
                    findings.push(Finding::protocol_drift(&w.plugin.name));
                }
            }
        }
    }
    Ok(())
}

/// Scan the `blockers` edges across `tasks/` for a cycle (§10, core-readable).
/// One finding names the loop; `bl update` unlinks an edge to break it.
fn circular_blockers(operating: &Path, findings: &mut Vec<Finding>) -> io::Result<()> {
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in entries(&operating.join("tasks"))? {
        if let Some(id) = task_id(&path) {
            if let Ok(task) = Task::parse(&fs::read_to_string(&path)?) {
                graph.insert(id, task.blockers.into_iter().map(|b| b.id).collect());
            }
        }
    }
    if let Some(cycle) = find_cycle(&graph) {
        findings.push(Finding::circular_blockers(&cycle));
    }
    Ok(())
}

/// The id a `tasks/<id>.md` path names — its `.md` stem (§3, the id IS the
/// filename). A non-`.md` path (or a subdir) is not a task file.
fn task_id(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    name.strip_suffix(".md").map(str::to_string)
}

/// The immediate paths under `dir`, or empty if `dir` is absent or not a
/// directory — the lean read both core checks share (a best-effort read op
/// surfaces no I/O error of its own; §16).
fn entries(dir: &Path) -> io::Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        paths.push(entry?.path());
    }
    Ok(paths)
}

/// The first cycle in the blocker graph, as the nodes on the loop in visit
/// order, or `None` if the edges form a DAG. A 3-state DFS: a node on the
/// current path (grey) closes a cycle; a fully-explored node (black) is skipped.
fn find_cycle(graph: &BTreeMap<String, Vec<String>>) -> Option<Vec<String>> {
    let mut explored = BTreeSet::new();
    for start in graph.keys() {
        let mut path = Vec::new();
        if let Some(cycle) = walk(start, graph, &mut explored, &mut path) {
            return Some(cycle);
        }
    }
    None
}

/// DFS from `node`: a back-edge into the live `path` returns the cycle slice;
/// an already-`explored` node short-circuits; otherwise recurse, then mark
/// `node` explored. An edge to an id with no task file has no neighbors.
fn walk(
    node: &str,
    graph: &BTreeMap<String, Vec<String>>,
    explored: &mut BTreeSet<String>,
    path: &mut Vec<String>,
) -> Option<Vec<String>> {
    if let Some(pos) = path.iter().position(|n| n == node) {
        return Some(path[pos..].to_vec());
    }
    if explored.contains(node) {
        return None;
    }
    path.push(node.to_string());
    if let Some(neighbors) = graph.get(node) {
        for next in neighbors {
            if let Some(cycle) = walk(next, graph, explored, path) {
                return Some(cycle);
            }
        }
    }
    path.pop();
    explored.insert(node.to_string());
    None
}

impl Finding {
    fn landing_unresolved(landing: &Path) -> Finding {
        Finding {
            drift: format!("landing checkout does not resolve: {}", landing.display()),
            fix: "bl prime (idempotently re-materializes a missing checkout)".into(),
        }
    }

    fn stale_change(path: &Path) -> Finding {
        Finding {
            drift: format!("stale change worktree (crashed-op debris): {}", path.display()),
            fix: format!(
                "git worktree remove {} — may hold uncommitted work, inspect first",
                path.display()
            ),
        }
    }

    fn dangling_plugin(name: &str) -> Finding {
        Finding {
            drift: format!("plugin {name} referenced but not installed here (bin/{name} missing)"),
            fix: "bl install (re-resolves the local binary)".into(),
        }
    }

    fn protocol_drift(name: &str) -> Finding {
        Finding {
            drift: format!("plugin {name}: protocol drift — no longer speaks balls protocol {PROTOCOL}"),
            fix: "bl install (re-validates the binary's protocol)".into(),
        }
    }

    fn circular_blockers(cycle: &[String]) -> Finding {
        let mut loop_path = cycle.to_vec();
        loop_path.push(cycle[0].clone());
        Finding {
            drift: format!("circular blockers: {}", loop_path.join(" -> ")),
            fix: "bl update (unlink one blockers edge to break the cycle)".into(),
        }
    }

    fn config_unresolved(err: &str) -> Finding {
        Finding {
            drift: format!("§4 config drift: {err}"),
            fix: "edit config/balls.toml (the malformed layer), then bl prime".into(),
        }
    }

    fn config_unusable(reason: &str) -> Finding {
        Finding {
            drift: format!("§4 config is unusable: {reason}"),
            fix: "correct config/balls.toml, then bl prime".into(),
        }
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.findings.is_empty() {
            return writeln!(f, "doctor: no core-owned drift detected");
        }
        writeln!(f, "doctor: {} core-owned finding(s)", self.findings.len())?;
        for finding in &self.findings {
            writeln!(f, "  - {}", finding.drift)?;
            writeln!(f, "    fix: {}", finding.fix)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
