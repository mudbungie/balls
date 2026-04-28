//! Pure-data tests for `Projection`: ownership/read-set semantics and
//! the `overlaps` predicate used by the dispatcher's registration-time
//! disjointness check (the dispatcher itself lands with bl-2bf7).
//! Lives alongside `participant_tests.rs` to keep that file under the
//! repo's per-file line cap.

use super::*;

#[test]
fn full_owns_every_canonical_field() {
    let p = Projection::full();
    assert_eq!(p.owns, Field::all());
    assert!(p.reads.is_empty());
    assert!(p.external_prefixes.is_empty());
}

#[test]
fn external_only_owns_only_its_prefix() {
    let p = Projection::external_only("jira");
    assert!(p.owns.is_empty());
    assert_eq!(p.reads, Field::all());
    assert!(p.external_prefixes.contains("jira"));
}

#[test]
fn overlap_detects_shared_owned_field() {
    let mut a = Projection::default();
    let mut b = Projection::default();
    a.owns.insert(Field::Status);
    b.owns.insert(Field::Status);
    assert!(a.overlaps(&b));
}

#[test]
fn overlap_detects_shared_external_prefix() {
    let a = Projection::external_only("jira");
    let b = Projection::external_only("jira");
    assert!(a.overlaps(&b));
}

#[test]
fn disjoint_external_prefixes_do_not_overlap() {
    let a = Projection::external_only("jira");
    let b = Projection::external_only("linear");
    assert!(!a.overlaps(&b));
}

#[test]
fn full_and_external_only_do_not_overlap() {
    // SPEC §5: a plugin's external slice does not collide with the
    // git-remote's full canonical projection because the plugin
    // doesn't *own* any canonical field.
    let git = Projection::full();
    let jira = Projection::external_only("jira");
    assert!(!git.overlaps(&jira));
}
