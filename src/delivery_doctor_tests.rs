//! Unit tests for the §16 delivery doctor: the [`audit`] partition (pure over
//! injected sets + path formula), its [`render`], and the two filesystem
//! gatherers ([`claimed_ids`] / [`materialized_ids`]) against throwaway dirs.

use super::*;

/// Derive a worktree path string under a fixed root — the [`audit`] `path_of`.
fn at_root(id: &str) -> String {
    format!("/wt/{id}")
}

#[test]
fn audit_partitions_claimed_balls_against_materialized_worktrees() {
    let claimed = BTreeSet::from(["bl-a".to_string(), "bl-b".to_string()]);
    let materialized = BTreeSet::from(["bl-b".to_string(), "bl-c".to_string()]);
    let findings = audit(&claimed, &materialized, &at_root);
    // bl-a: claimed, no worktree → missing; bl-c: worktree, no claim → orphan;
    // bl-b is aligned and yields nothing.
    assert_eq!(findings.len(), 2);
    assert!(findings[0].drift.contains("claimed ball bl-a has no code worktree: /wt/bl-a"));
    assert_eq!(findings[0].fix, "bl prime (idempotently re-materializes the worktree)");
    assert!(findings[1].drift.contains("orphan code worktree (no live claim): /wt/bl-c"));
    assert!(findings[1].fix.contains("git worktree remove /wt/bl-c"));
}

#[test]
fn audit_finds_no_drift_when_claims_and_worktrees_align() {
    let aligned = BTreeSet::from(["bl-a".to_string()]);
    assert!(audit(&aligned, &aligned, &at_root).is_empty());
}

#[test]
fn render_prints_a_clean_line_when_there_is_no_drift() {
    assert_eq!(render(&[]), "delivery: no code-worktree drift detected\n");
}

#[test]
fn render_lists_a_header_then_each_finding() {
    let findings = audit(&BTreeSet::from(["bl-a".to_string()]), &BTreeSet::new(), &at_root);
    let out = render(&findings);
    assert!(out.starts_with("delivery: 1 code-worktree finding(s)\n"));
    assert!(out.contains("  - claimed ball bl-a has no code worktree: /wt/bl-a\n"));
    assert!(out.contains("    fix: bl prime (idempotently re-materializes the worktree)\n"));
}

#[test]
fn materialized_ids_lists_only_subdirectories() {
    let tmp = tempfile::TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("bl-a")).unwrap();
    fs::create_dir(tmp.path().join("bl-b")).unwrap();
    fs::write(tmp.path().join("stray.lock"), "x").unwrap(); // a file, not a worktree
    assert_eq!(materialized_ids(tmp.path()).unwrap(), BTreeSet::from(["bl-a".to_string(), "bl-b".to_string()]));
}

#[test]
fn materialized_ids_on_an_absent_territory_is_empty() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert!(materialized_ids(&tmp.path().join("nope")).unwrap().is_empty());
}
