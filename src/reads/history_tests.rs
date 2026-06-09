//! Tests for the recency walk over `balls/tasks` history — single-id
//! reconstruction ([`resolve_dead`]) and the dead-set enumeration
//! ([`dead_balls`]), exercised against a real throwaway git store.

use super::*;
use crate::reads::test_support::{git_store, task};

#[test]
fn resolve_dead_reconstructs_a_closed_ball_from_history() {
    let s = git_store();
    let mut t = task("Refactor", 100);
    t.body = "the plan".into();
    s.create("bl-1", &t, 100).retire("bl-1", "close", 500);

    let dead = resolve_dead(s.dir(), "bl-1").unwrap().unwrap();
    assert_eq!(dead.id, "bl-1");
    assert_eq!(dead.task.title, "Refactor");
    assert_eq!(dead.task.body, "the plan"); // frontmatter+body from the deletion's parent
    assert_eq!(dead.retired_at, 500); // the deletion commit's date
}

#[test]
fn resolve_dead_reconstructs_a_dropped_ball_the_same_as_a_closed_one() {
    // A `drop` retirement reconstructs identically — no distinct status survives
    // the collapse; the `bl-op: drop` trailer stays git bedrock, never a field.
    let s = git_store();
    s.create("bl-2", &task("Abandoned", 1), 1).retire("bl-2", "drop", 9);
    let dead = resolve_dead(s.dir(), "bl-2").unwrap().unwrap();
    assert_eq!(dead.task.title, "Abandoned");
    assert_eq!(dead.retired_at, 9);
}

#[test]
fn resolve_dead_is_none_when_the_id_was_never_deleted() {
    let s = git_store();
    s.create("bl-live", &task("Alive", 1), 1); // born, never retired
    // No deletion in history ⇒ no dead incarnation (the empty-log path).
    assert!(resolve_dead(s.dir(), "bl-live").unwrap().is_none());
    assert!(resolve_dead(s.dir(), "bl-never").unwrap().is_none());
}

#[test]
fn resolve_dead_takes_the_newest_incarnation_of_a_reused_id() {
    let s = git_store();
    // Same id lived twice: closed as "First", then re-created and dropped as "Second".
    s.create("bl-r", &task("First", 1), 1).retire("bl-r", "close", 2);
    s.create("bl-r", &task("Second", 3), 3).retire("bl-r", "drop", 4);

    let dead = resolve_dead(s.dir(), "bl-r").unwrap().unwrap();
    assert_eq!(dead.task.title, "Second"); // most-recent-down
    assert_eq!(dead.retired_at, 4);
}

#[test]
fn resolve_dead_surfaces_a_corrupt_historical_file_as_an_error() {
    let s = git_store();
    s.create_raw("bl-bad", "not valid frontmatter", 1).retire("bl-bad", "close", 2);
    assert!(resolve_dead(s.dir(), "bl-bad").is_err());
}

#[test]
fn resolve_dead_errors_when_the_store_is_not_a_git_repo() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert!(resolve_dead(&tmp.path().join("nope"), "bl-x").is_err());
}

#[test]
fn dead_balls_enumerates_every_dead_ball_newest_first() {
    let s = git_store();
    s.create("bl-a", &task("A", 1), 1).retire("bl-a", "close", 10);
    s.create("bl-b", &task("B", 2), 2).retire("bl-b", "drop", 20);

    let live = Catalog::load(s.dir()).unwrap();
    let dead = dead_balls(s.dir(), &live).unwrap();
    let ids: Vec<&str> = dead.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["bl-b", "bl-a"]); // newest deletion first
}

#[test]
fn dead_balls_excludes_an_id_that_is_live_again() {
    let s = git_store();
    s.create("bl-x", &task("X", 1), 1).retire("bl-x", "close", 2);
    s.create("bl-x", &task("X-again", 3), 3); // re-created, still live

    let live = Catalog::load(s.dir()).unwrap();
    let dead = dead_balls(s.dir(), &live).unwrap();
    assert!(dead.is_empty()); // resolves live, so not in the dead set
}

#[test]
fn dead_balls_dedupes_a_twice_deleted_id_to_its_newest() {
    let s = git_store();
    s.create("bl-d", &task("D1", 1), 1).retire("bl-d", "close", 2);
    s.create("bl-d", &task("D2", 3), 3).retire("bl-d", "drop", 4);

    let live = Catalog::load(s.dir()).unwrap();
    let dead = dead_balls(s.dir(), &live).unwrap();
    assert_eq!(dead.len(), 1); // one row, not two deletions
    assert_eq!(dead[0].task.title, "D2");
}
