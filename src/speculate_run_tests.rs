//! Unit tests for [`crate::speculate_run`] — the whole pass against fixture
//! repos with a stub gate, proving the chain rules the design states: strict
//! head-first order, stop on conflict/fail/spent, sweep of the unsealed, and
//! the no-leftover invariants.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::run;
use crate::speculate_queue::{enqueue, queue};

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo whose gate files exist (the fingerprint reads them), with two
/// cleanly-stacking work branches and one that conflicts with main.
struct Fx {
    _tmp: TempDir,
    root: PathBuf,
    scratch: PathBuf,
    territory: PathBuf,
    gate: PathBuf,
    log: PathBuf,
}

fn fx(gate_exit: &str) -> Fx {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join("scripts")).unwrap();
    for rel in ["scripts/pre-commit", "scripts/check-line-lengths.sh", "scripts/check-coverage.sh"] {
        fs::write(root.join(rel), "#!/bin/sh\n").unwrap();
    }
    fs::write(root.join("Makefile"), "all:\n").unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("shared"), "line\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    let base = git(&root, &["rev-parse", "HEAD"]);
    for (branch, file) in [("work/a", "fa"), ("work/b", "fb")] {
        git(&root, &["checkout", "-q", "-b", branch, &base]);
        fs::write(root.join(file), branch).unwrap();
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", branch]);
    }
    git(&root, &["checkout", "-q", "-b", "work/hostile", &base]);
    fs::write(root.join("shared"), "hostile\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "hostile"]);
    git(&root, &["checkout", "-q", "main"]);
    fs::write(root.join("shared"), "moved\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "moved"]);
    let log = tmp.path().join("gate.log");
    let gate = tmp.path().join("gate.sh");
    fs::write(&gate, format!("#!/bin/sh\npwd >> {}\nexit {gate_exit}\n", log.display())).unwrap();
    fs::set_permissions(&gate, fs::Permissions::from_mode(0o755)).unwrap();
    Fx {
        scratch: tmp.path().join("scratch"),
        territory: tmp.path().join("territory"),
        _tmp: tmp,
        root,
        gate,
        log,
    }
}

fn pass(f: &Fx, builds: usize) -> Vec<String> {
    run(
        &f.root,
        &f.scratch,
        &f.territory,
        "rustc test",
        "main",
        f.gate.to_str().unwrap(),
        builds,
    )
    .unwrap()
}

fn gate_runs(f: &Fx) -> usize {
    fs::read_to_string(&f.log).map_or(0, |s| s.lines().count())
}

#[test]
fn builds_head_first_then_hits_and_the_second_pass_spends_nothing() {
    let f = fx("0");
    enqueue(&f.root, "a", Some("2026-01-01T10:00:00Z")).unwrap();
    enqueue(&f.root, "b", Some("2026-01-01T11:00:00Z")).unwrap();
    let report = pass(&f, 10);
    assert_eq!(report.len(), 2, "{report:?}");
    assert!(report[0].starts_with("built a ") && report[0].ends_with(" pass"), "{report:?}");
    assert!(report[1].starts_with("built b "), "{report:?}");
    assert_eq!(gate_runs(&f), 2);
    let again = pass(&f, 10);
    assert!(again[0].starts_with("hit a ") && again[1].starts_with("hit b "), "{again:?}");
    assert_eq!(gate_runs(&f), 2, "hits spend no gates");
    let listing = git(&f.root, &["worktree", "list", "--porcelain"]);
    assert_eq!(listing.matches("worktree ").count(), 1, "no build worktrees survive");
}

#[test]
fn budget_defers_and_a_failing_gate_stops_the_chain() {
    let f = fx("1");
    enqueue(&f.root, "a", Some("2026-01-01T10:00:00Z")).unwrap();
    enqueue(&f.root, "b", Some("2026-01-01T11:00:00Z")).unwrap();
    let report = pass(&f, 0);
    assert_eq!(report, vec!["deferred a (builds spent)"], "builds=0 is plan-only");
    let report = pass(&f, 10);
    assert!(report[0].starts_with("built a ") && report[0].contains("FAIL"), "{report:?}");
    assert_eq!(report.len(), 1, "a failing prefix ends the pass: {report:?}");
    let report = pass(&f, 10);
    assert!(report[0].starts_with("fail a "), "the recorded FAIL stops later passes too");
    assert_eq!(gate_runs(&f), 1, "the fail was built once, never again");
}

#[test]
fn conflicts_stop_the_chain_and_the_unsealed_are_swept() {
    let f = fx("0");
    enqueue(&f.root, "hostile", Some("2026-01-01T10:00:00Z")).unwrap();
    enqueue(&f.root, "a", Some("2026-01-01T11:00:00Z")).unwrap();
    let report = pass(&f, 10);
    assert!(report[0].starts_with("conflict hostile"), "{report:?}");
    assert_eq!(report.len(), 1, "nothing builds past a conflict");
    git(&f.root, &["branch", "-D", "work/hostile"]);
    let report = pass(&f, 10);
    assert_eq!(report[0], "swept hostile (unsealed)", "{report:?}");
    assert!(report[1].starts_with("built a "), "the queue heals past the swept: {report:?}");
    assert_eq!(queue(&f.root).unwrap().len(), 1, "the stale tag is gone");
}

#[test]
fn a_missing_landing_branch_is_loud() {
    let f = fx("0");
    let err = run(&f.root, &f.scratch, &f.territory, "rustc test", "ghost", "true", 1).unwrap_err();
    assert!(err.to_string().contains("rev-parse ghost"), "{err}");
}
