//! Delivery-gate tests (bl-ee85): `deliver` runs the project repo's own
//! pre-commit hook — once, on the reintegrated tree it is about to land — and
//! a failure aborts the close before anything reaches integration.

use super::tests::{project, tip};
use super::*;
use std::os::unix::fs::PermissionsExt;

/// Install `script` as the project repo's `pre-commit` hook (the shared
/// `.git/hooks` every linked worktree resolves), `mode`-permissioned.
fn install_hook(root: &Path, script: &str, mode: u32) {
    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, script).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn a_passing_gate_delivers_and_runs_in_the_worktree() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    // The hook proves where it ran: it requires the work's own file in $PWD.
    install_hook(&root, "#!/bin/sh\ntest -f feature.txt\n", 0o755);

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]");
}

#[test]
fn a_failing_gate_aborts_the_delivery_before_integration_moves() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "broken\n").unwrap();
    install_hook(&root, "#!/bin/sh\nexit 1\n", 0o755);

    let err = p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]").unwrap_err();
    assert!(err.to_string().contains("delivery gate"), "{err}");
    assert_eq!(tip(&root), "seed"); // integration untouched
    // The work survives the abort: captured on the branch (--no-verify, so the
    // failing hook could not block the capture — the gate runs ONCE, here).
    let subject = Project::run(&root, &["log", "-1", "--format=%s", "work/bl-x"]).unwrap();
    assert_eq!(subject.trim(), "Add feature [bl-x]");
}

#[test]
fn a_non_executable_hook_is_ignored_gits_rule() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    install_hook(&root, "#!/bin/sh\nexit 1\n", 0o644); // would fail, but is not a hook

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]");
}

#[test]
fn the_gate_checks_the_reintegrated_tree_when_integration_moved() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    // Integration moves AFTER the claim — the gate must see BOTH sides, i.e.
    // the merged tree that will actually land, not the stale branch tip.
    fs::write(root.join("late.txt"), "landed meanwhile\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "late main edit"]).unwrap();
    install_hook(&root, "#!/bin/sh\ntest -f feature.txt && test -f late.txt\n", 0o755);

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]");
    // Still ONE squash commit, parented on the moved integration tip.
    assert_eq!(Project::run(&root, &["show", "main:late.txt"]).unwrap(), "landed meanwhile\n");
    assert_eq!(Project::run(&root, &["rev-list", "--count", "main"]).unwrap().trim(), "3");
}

#[test]
fn a_reintegration_that_dissolves_the_diff_skips_gate_and_squash() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "same\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "work copy"]).unwrap();
    // The identical change already landed on integration (e.g. via a sibling)
    // ALONGSIDE more — so the trees differ before the fold (no early empty-
    // deliverable exit) and converge to integration's after it.
    fs::write(root.join("feature.txt"), "same\n").unwrap();
    fs::write(root.join("late.txt"), "more\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "already landed"]).unwrap();
    install_hook(&root, "#!/bin/sh\nexit 1\n", 0o755); // must never run

    p.deliver(&wt, "work/bl-x", "main", "dup [bl-x]").unwrap();
    assert_eq!(tip(&root), "already landed"); // no delivery commit minted
}

#[test]
fn deliver_rematerializes_an_absent_worktree_to_gate_in() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.release(&wt).unwrap(); // committed branch, no worktree on this box
    install_hook(&root, "#!/bin/sh\ntest -f feature.txt\n", 0o755);

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]");
    assert!(wt.exists()); // recreated for the gate; close.post releases it
}
