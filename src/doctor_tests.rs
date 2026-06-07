//! Tests for §16 base doctor. Each builds a throwaway clone bundle in a tempdir
//! and drives [`audit`] against real on-disk drift; the protocol probe is a fake
//! so no plugin binary is spawned. Task files are written via [`Task::to_markdown`]
//! so the checks stay independent of the frontmatter serialization format.

use super::*;
use crate::layout::{CloneDir, Xdg};
use crate::registry::Registry;
use crate::task::{Blocker, On, Task};
use std::fs;

/// A clone bundle rooted in a fresh tempdir. The two checkouts are absent until
/// [`with_checkouts`] makes them real.
fn fixture() -> (tempfile::TempDir, CloneDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let xdg = Xdg::with(tmp.path(), None, tmp.path().to_str());
    let clone = xdg.clone_dir(Path::new("/proj"));
    fs::create_dir_all(clone.root()).unwrap();
    (tmp, clone)
}

/// Make the LANDING and STORE checkouts real (the store with a `tasks/` dir);
/// return their paths. The registry + config live on the landing, tasks on the
/// store (§2), so the audit reads each from its own checkout.
fn with_checkouts(clone: &CloneDir) -> (PathBuf, PathBuf) {
    let (landing, store) = (clone.landing(), clone.store());
    fs::create_dir_all(landing.join("config")).unwrap();
    fs::create_dir_all(store.join("tasks")).unwrap();
    (landing, store)
}

/// An absent XDG user-config path — the §4 layer contributes nothing, so the
/// config check sees only the landing's own `config/balls.toml`.
fn no_user_config() -> PathBuf {
    PathBuf::from("/nonexistent/balls/config.toml")
}

/// Write `body` to the landing's `config/balls.toml` — the §4 layer doctor reads.
fn with_config(landing: &Path, body: &str) {
    fs::create_dir_all(landing.join("config")).unwrap();
    fs::write(landing.join("config").join("balls.toml"), body).unwrap();
}

/// Write `tasks/<id>.md` (on the store) declaring claim-blockers on each id.
fn write_task(store: &Path, id: &str, blockers: &[&str]) {
    let task = Task {
        title: id.into(),
        blockers: blockers.iter().map(|b| Blocker { id: (*b).into(), on: On::Claim }).collect(),
        ..Default::default()
    };
    fs::write(store.join("tasks").join(format!("{id}.md")), task.to_markdown()).unwrap();
}

// These fakes always succeed; the `io::Result` is the probe seam's signature.
#[allow(clippy::unnecessary_wraps)]
fn speaks_current(_: &Path) -> io::Result<Protocol> {
    Ok(Protocol { protocol: vec![PROTOCOL], ops: Vec::new() })
}
#[allow(clippy::unnecessary_wraps)]
fn speaks_other(_: &Path) -> io::Result<Protocol> {
    Ok(Protocol { protocol: vec![PROTOCOL + 1], ops: Vec::new() })
}
fn probe_fails(_: &Path) -> io::Result<Protocol> {
    Err(io::Error::other("self-describe failed"))
}

/// Does any finding's drift line contain `needle`?
fn has(report: &Report, needle: &str) -> bool {
    report.findings.iter().any(|f| f.drift.contains(needle))
}

#[test]
fn a_clean_clone_yields_no_findings() {
    let (_t, clone) = fixture();
    with_checkouts(&clone);
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    assert!(report.findings.is_empty());
    assert_eq!(report.to_string(), "doctor: no core-owned drift detected\n");
}

#[test]
fn an_unresolved_landing_is_a_finding_fixed_by_prime() {
    let (_t, clone) = fixture(); // landing never created
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    assert!(has(&report, "landing checkout does not resolve"));
    assert!(report.findings[0].fix.contains("bl prime"));
}

#[test]
fn a_leftover_change_worktree_is_named_with_its_removal() {
    let (_t, clone) = fixture();
    with_checkouts(&clone);
    let debris = clone.root().join("changes").join("dead-uuid");
    fs::create_dir_all(&debris).unwrap();
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    assert!(has(&report, "stale change worktree"));
    let finding = report.findings.iter().find(|f| f.drift.contains("stale")).unwrap();
    assert!(finding.fix.contains("git worktree remove"));
    assert!(finding.fix.contains("dead-uuid"));
}

#[test]
fn a_wired_but_uninstalled_plugin_is_a_dangle_fixed_by_install() {
    let (_t, clone) = fixture();
    let (landing, _store) = with_checkouts(&clone);
    Registry::at(&landing).link("close", "pre", 0, "tracker").unwrap(); // no bind → dangling
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    assert!(has(&report, "tracker referenced but not installed"));
    let finding = report.findings.iter().find(|f| f.drift.contains("tracker")).unwrap();
    assert!(finding.fix.contains("bl install"));
}

#[test]
fn a_plugin_that_no_longer_speaks_the_protocol_is_drift() {
    let (t, clone) = fixture();
    let (landing, _store) = with_checkouts(&clone);
    let reg = Registry::at(&landing);
    let bin = t.path().join("plugin-bin");
    fs::write(&bin, "x").unwrap();
    reg.link("close", "pre", 0, "tracker").unwrap();
    reg.bind("tracker", &bin).unwrap();
    let report = audit(&clone, &no_user_config(), &speaks_other).unwrap();
    assert!(has(&report, "protocol drift"));
    assert!(report.findings[0].fix.contains("bl install"));
}

#[test]
fn a_plugin_whose_self_describe_fails_is_also_drift() {
    let (t, clone) = fixture();
    let (landing, _store) = with_checkouts(&clone);
    let reg = Registry::at(&landing);
    let bin = t.path().join("plugin-bin");
    fs::write(&bin, "x").unwrap();
    reg.link("close", "pre", 0, "tracker").unwrap();
    reg.bind("tracker", &bin).unwrap();
    let report = audit(&clone, &no_user_config(), &probe_fails).unwrap();
    assert!(has(&report, "protocol drift"));
}

#[test]
fn an_installed_plugin_that_still_speaks_is_clean() {
    let (t, clone) = fixture();
    let (landing, _store) = with_checkouts(&clone);
    let reg = Registry::at(&landing);
    let bin = t.path().join("plugin-bin");
    fs::write(&bin, "x").unwrap();
    reg.link("close", "pre", 0, "tracker").unwrap();
    reg.bind("tracker", &bin).unwrap();
    assert!(audit(&clone, &no_user_config(), &speaks_current).unwrap().findings.is_empty());
}

#[test]
fn a_blocker_cycle_is_reported_with_the_loop_and_an_update_fix() {
    let (_t, clone) = fixture();
    let (_landing, store) = with_checkouts(&clone);
    write_task(&store, "bl-a", &["bl-b"]);
    write_task(&store, "bl-b", &["bl-a"]);
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    let finding = report.findings.iter().find(|f| f.drift.contains("circular")).unwrap();
    assert!(finding.drift.contains("bl-a -> bl-b -> bl-a"));
    assert!(finding.fix.contains("bl update"));
}

#[test]
fn a_dag_with_a_diamond_and_a_dangling_edge_has_no_cycle() {
    let (_t, clone) = fixture();
    let (_landing, store) = with_checkouts(&clone);
    // a → {b, c}; b → d; c → d (d re-visited → black short-circuit);
    // d → x where x has no task file (a blocker edge with no node).
    write_task(&store, "bl-a", &["bl-b", "bl-c"]);
    write_task(&store, "bl-b", &["bl-d"]);
    write_task(&store, "bl-c", &["bl-d"]);
    write_task(&store, "bl-d", &["bl-x"]);
    // A non-`.md` file in tasks/ is not a task and is skipped.
    fs::write(store.join("tasks").join("notes.txt"), "scratch").unwrap();
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    assert!(!has(&report, "circular"));
}

#[test]
fn a_malformed_config_layer_is_drift_fixed_by_editing_and_priming() {
    let (_t, clone) = fixture();
    let (landing, _store) = with_checkouts(&clone);
    with_config(&landing, "tasks_branch = [not valid toml\n");
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    let finding = report.findings.iter().find(|f| f.drift.contains("§4 config drift")).unwrap();
    assert!(finding.fix.contains("config/balls.toml"));
    assert!(finding.fix.contains("bl prime"));
}

#[test]
fn an_empty_tasks_branch_is_an_unusable_config() {
    let (_t, clone) = fixture();
    let (landing, _store) = with_checkouts(&clone);
    with_config(&landing, "tasks_branch = \"\"\n");
    let report = audit(&clone, &no_user_config(), &speaks_current).unwrap();
    assert!(has(&report, "tasks_branch is empty"));
}

#[test]
fn a_valid_custom_config_is_clean() {
    let (_t, clone) = fixture();
    let (landing, _store) = with_checkouts(&clone);
    with_config(&landing, "tasks_branch = \"work\"\n");
    assert!(audit(&clone, &no_user_config(), &speaks_current).unwrap().findings.is_empty());
}

#[test]
fn the_report_renders_each_finding_with_its_fix() {
    let (_t, clone) = fixture(); // unresolved landing → exactly one finding
    let rendered = audit(&clone, &no_user_config(), &speaks_current).unwrap().to_string();
    assert!(rendered.starts_with("doctor: 1 core-owned finding(s)\n"));
    assert!(rendered.contains("  - landing checkout does not resolve"));
    assert!(rendered.contains("    fix: bl prime"));
}
