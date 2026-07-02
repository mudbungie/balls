//! Tests for the §9 journal walk — oldest-first entries off `tasks/<id>.md`
//! history, the §5 note render (body minus git's own trailer block), and the
//! non-balls-commit fallbacks (author for actor, whole body as the note).

use super::*;
use crate::reads::test_support::{git_store, task};

#[test]
fn section_renders_oldest_first_with_ops_actors_and_notes() {
    let s = git_store();
    let t = task("One", 100);
    s.create("bl-1", &t, 100).note("bl-1", &t, "waiting on upstream\n\nsecond paragraph", 200);
    s.retire("bl-1", "close", 300);
    let out = section(s.dir(), "bl-1").unwrap();
    assert_eq!(
        out,
        "  journal\n\
         \x20   1970-01-01T00:01:40Z  create   t\n\
         \x20   1970-01-01T00:03:20Z  update   t\n\
         \x20     waiting on upstream\n\
         \n\
         \x20     second paragraph\n\
         \x20   1970-01-01T00:05:00Z  close    t\n"
    );
}

#[test]
fn section_is_empty_for_a_path_with_no_history() {
    // A store with commits, none touching this id — the walk yields nothing
    // and the section stays "" (show then folds no journal at all).
    let s = git_store();
    s.create("bl-other", &task("Other", 1), 1);
    assert_eq!(section(s.dir(), "bl-none").unwrap(), "");
}

#[test]
fn section_errors_when_the_store_is_not_walkable() {
    // The same contract as the dead-ball walk: a broken store surfaces, it is
    // not silently an empty journal.
    assert!(section(std::path::Path::new("/balls-no-such-store"), "bl-1").is_err());
}

#[test]
fn a_non_balls_commit_falls_back_to_the_git_author_and_whole_body() {
    // A hand commit touching the file carries no §5 trailer block: the actor
    // falls back to the git author (pinned — a hook run exports the invoker's
    // GIT_AUTHOR_NAME) and the whole body IS the note — history is total,
    // never filtered.
    let s = git_store();
    s.create("bl-1", &task("One", 1), 1);
    std::fs::write(s.dir().join("tasks/bl-1.md"), "+++\ntitle = \"One\"\ncreated = 1\nupdated = 2\n+++\n")
        .unwrap();
    crate::git::run(s.dir(), &["add", "-A"], None).unwrap();
    let commit = ["commit", "-qm", "hand edit\n\nout-of-band fixup", "--author=hand <h@x>"];
    crate::git::run(s.dir(), &commit, None).unwrap();
    let out = section(s.dir(), "bl-1").unwrap();
    let hand = out.lines().nth(2).unwrap();
    assert!(hand.ends_with("  hand"), "author fallback, no op: {hand:?}");
    assert!(out.ends_with("      out-of-band fixup\n"), "body as note: {out}");
}
