//! Tests for claim-age: the newest `bl-op: claim` commit's timestamp (§9
//! recency — an unclaim/reclaim cycle resolves to the LIVE claim) and the
//! coarse minutes/hours/days humanizer.

use super::*;
use crate::reads::test_support::{git_store, task};

/// A `Held` ball whose frontmatter carries `claimant`.
fn held(at: i64) -> crate::task::Task {
    let mut t = task("Held", at);
    t.claimant = Some("worker".into());
    t
}

#[test]
fn claimed_at_reads_the_newest_claim_commit() {
    // create → claim → unclaim(update) → reclaim: newest-first `-1` wins.
    let s = git_store();
    s.create("bl-1", &task("Held", 100), 100).claim("bl-1", &held(100), 200);
    s.note("bl-1", &task("Held", 100), "released", 300).claim("bl-1", &held(100), 400);
    assert_eq!(claimed_at(s.dir(), "bl-1").unwrap(), Some(400));
}

#[test]
fn claimed_at_is_none_without_a_claim_commit() {
    // A ball only ever `create`d (claimant hand-set, no claim op) has no claim
    // time; nor does an id the store never saw.
    let s = git_store();
    s.create("bl-1", &held(1), 1);
    assert_eq!(claimed_at(s.dir(), "bl-1").unwrap(), None);
    assert_eq!(claimed_at(s.dir(), "bl-none").unwrap(), None);
}

#[test]
fn claimed_at_errors_when_the_store_is_not_walkable() {
    // Same contract as the journal walk: a broken store surfaces as an error,
    // never a silent "no claim".
    assert!(claimed_at(std::path::Path::new("/balls-no-such-store"), "bl-1").is_err());
}

#[test]
fn humanize_picks_one_coarse_unit() {
    assert_eq!(humanize(0), "0m");
    assert_eq!(humanize(59), "0m"); // sub-minute floors to minutes
    assert_eq!(humanize(3 * 60), "3m");
    assert_eq!(humanize(3 * 3_600), "3h");
    assert_eq!(humanize(2 * 86_400 + 5), "2d");
    assert_eq!(humanize(-10), "0m"); // clock skew floors at zero
}
