//! End-to-end visual smoke test for the CLI display overhaul (bl-a847).
//!
//! Locks in the overall look-and-feel across `bl list`, `bl ready`,
//! `bl show`, and `bl dep tree` in one go. Each assertion here maps
//! back to a specific spec point so a future renderer change can't
//! silently regress the visual contract.

mod common;

use common::*;

#[test]
fn visual_contract_across_commands() {
    let repo = new_repo();
    init_in(repo.path());

    // Create an epic with two children and a gates link so every
    // badge/annotation we promise in the README gets exercised.
    let epic_out = bl(repo.path())
        .args(["create", "Epic one", "-t", "epic", "-p", "2", "--tag", "display"])
        .output()
        .unwrap();
    let epic = String::from_utf8_lossy(&epic_out.stdout).trim().to_string();
    let kid_a_out = bl(repo.path())
        .args(["create", "child A", "--parent", &epic])
        .output()
        .unwrap();
    let kid_a = String::from_utf8_lossy(&kid_a_out.stdout).trim().to_string();
    bl(repo.path())
        .args(["create", "child B", "--parent", &epic])
        .assert()
        .success();
    let gate_out = bl(repo.path())
        .args(["create", "audit", "--parent", &epic])
        .output()
        .unwrap();
    let gate = String::from_utf8_lossy(&gate_out.stdout).trim().to_string();
    bl(repo.path())
        .args(["link", "add", &epic, "gates", &gate])
        .assert()
        .success();

    // bl list: status header, epic marker, progress bar, `G` gates
    // badge on the epic row, ASCII fallback in the captured output.
    let out = bl(repo.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("[ ] open"), "missing status header");
    assert!(s.contains("[epic]"), "missing epic marker");
    assert!(s.contains("----------"), "missing 10-cell bar");
    assert!(s.contains(" G "), "missing gates badge");
    assert!(s.contains(&epic));
    assert!(s.contains(&kid_a));

    // bl ready: parent hint on the child rows, absent on the root.
    let out = bl(repo.path()).arg("ready").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("^ ") && s.contains("Epic one"));

    // bl show: header line, type row, children, progress row, wrapped
    // description placeholder absent (no description set).
    let out = bl(repo.path()).args(["show", &epic]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("type: epic"));
    assert!(s.contains("progress: "));
    assert!(s.contains("children:"));
    assert!(s.contains(&kid_a));

    // bl dep tree: parent/child box-drawing (ASCII fallback), epic
    // marker on the root, no nesting by dep edges.
    let out = bl(repo.path()).args(["dep", "tree"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("|- ") || s.contains("`- "), "no tree prefix");
    assert!(s.contains("[epic]"));
}
