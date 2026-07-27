//! Delivery-gate tests (bl-ee85): `deliver` runs the project repo's own
//! pre-commit hook — once, on the reintegrated tree it is about to land — and
//! a failure aborts the close before anything reaches integration.

#![cfg(unix)]

use super::tests::{project, tip};
use super::*;
use crate::delivery::Repo;
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

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]");
}

#[test]
fn a_failing_gate_aborts_the_delivery_before_integration_moves() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "broken\n").unwrap();
    install_hook(&root, "#!/bin/sh\nexit 1\n", 0o755);

    let err = p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap_err();
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

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
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

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
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

    p.deliver(&wt, "work/bl-x", "main", "dup [bl-x]", "[bl-x]").unwrap();
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

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]");
    assert!(wt.exists()); // recreated for the gate; close.post releases it
}

/// A pre-commit hook script that lands a sibling commit on `main` DURING the
/// gate — the deterministic mid-gate advance (bl-8b89) — exactly once (`guard`
/// marks it done, so a retried close gates quietly) and with `--no-verify` so
/// its own commit never re-enters this hook.
fn sibling_landing_hook(root: &Path, guard: &Path, file: &str, content: &str) -> String {
    format!(
        "#!/bin/sh\nif [ ! -e \"{g}\" ]; then\n  : > \"{g}\"\n  printf '%s\\n' '{content}' > \"{r}/{file}\"\n  \
         git -C \"{r}\" add -A\n  git -C \"{r}\" commit -q --no-verify -m 'sibling landed'\nfi\n",
        g = guard.display(),
        r = root.display()
    )
}

#[test]
fn a_mid_gate_advance_is_one_clean_cas_rejection_not_a_resurrection_abort() {
    // bl-8b89 mode 1: a sibling close lands mid-gate touching a path this
    // branch never authored. Comparing the squash against the LIVE tip made
    // that path read as excess — a false no-resurrection abort NAMING the
    // sibling's innocent path. Against the pinned fold base the invariant
    // passes and the delivery reaches its CAS, which rejects cleanly in the
    // bl-a3bb re-run voice.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    let guard = tmp.path().join("advanced");
    install_hook(&root, &sibling_landing_hook(&root, &guard, "sibling.txt", "sibling"), 0o755);

    let err = p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("moved under the delivery"), "{msg}");
    assert!(msg.contains("Re-run `bl close`"), "{msg}");
    assert!(!msg.contains("no-resurrection"), "{msg}");
    // Nothing overwritten: the sibling's landing IS main's tip, the work never landed.
    assert_eq!(tip(&root), "sibling landed");
    assert_eq!(Project::run(&root, &["show", "main:sibling.txt"]).unwrap(), "sibling\n");
    assert!(!Project::ok(&root, &["cat-file", "-e", "main:feature.txt"]).unwrap());

    // The retried close re-folds the moved tip and delivers onto it (§14
    // converge-on-retry) — BOTH landings survive; the guard keeps the hook quiet.
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]");
    assert_eq!(Project::run(&root, &["show", "main:sibling.txt"]).unwrap(), "sibling\n");
    assert_eq!(Project::run(&root, &["show", "main:feature.txt"]).unwrap(), "shipped\n");
}

#[test]
fn a_mid_gate_subset_advance_is_rejected_never_silently_reverted() {
    // bl-8b89 mode 2 (the corruption): the sibling's mid-gate landing touches
    // ONLY a path this branch also authored, so no path reads as excess even
    // against the live tip. With a post-gate re-read as the CAS old-value the
    // swap then succeeded against the post-move tip and the squash — computed
    // from the pre-move fold — SILENTLY reverted the sibling's landed change.
    // The pinned base turns that into the same clean CAS rejection.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("seed.txt"), "work edit\n").unwrap(); // the shared path
    let guard = tmp.path().join("advanced");
    install_hook(&root, &sibling_landing_hook(&root, &guard, "seed.txt", "sibling edit"), 0o755);

    let err = p.deliver(&wt, "work/bl-x", "main", "Edit seed [bl-x]", "[bl-x]").unwrap_err();
    assert!(err.to_string().contains("moved under the delivery"), "{err}");
    // THE assertion this test exists for: the sibling's landed content
    // survives on main — nothing was silently reverted.
    assert_eq!(Project::run(&root, &["show", "main:seed.txt"]).unwrap(), "sibling edit\n");
    assert_eq!(tip(&root), "sibling landed");

    // The retry surfaces the overlap as an ORDINARY delivery conflict for the
    // agent to resolve by hand — never a silent winner.
    let retry = p.deliver(&wt, "work/bl-x", "main", "Edit seed [bl-x]", "[bl-x]").unwrap_err();
    assert!(retry.to_string().contains("delivery conflict"), "{retry}");
    assert_eq!(Project::run(&root, &["show", "main:seed.txt"]).unwrap(), "sibling edit\n");
}
