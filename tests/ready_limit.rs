//! `bl ready --limit N` cap behavior, both text and JSON.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn ready_limit_truncates_text() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "alpha");
    let b = create_task(repo.path(), "beta");
    let c = create_task(repo.path(), "gamma");
    let out = bl(repo.path())
        .args(["ready", "--limit", "2"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let shown = [&a, &b, &c].iter().filter(|id| s.contains(**id)).count();
    assert_eq!(shown, 2);
    assert!(s.contains("... and 1 more"), "expected footer, got: {s}");
}

#[test]
fn ready_limit_text_no_footer_when_not_truncated() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "a");
    let out = bl(repo.path())
        .args(["ready", "--limit", "5"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(!s.contains("more"), "unexpected footer: {s}");
}

#[test]
fn ready_limit_truncates_json() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "a");
    create_task(repo.path(), "b");
    create_task(repo.path(), "c");
    let out = bl(repo.path())
        .args(["ready", "--limit", "2", "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 2);
}

#[test]
fn ready_limit_zero_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "a");
    bl(repo.path())
        .args(["ready", "--limit", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(">= 1"));
}

#[test]
fn ready_limit_negative_rejected_by_clap() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "a");
    bl(repo.path())
        .args(["ready", "--limit", "-1"])
        .assert()
        .failure();
}
