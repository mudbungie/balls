//! §9 `reopen` front-door dispatch tests — the history reconstruction the verb
//! authors from, its two refusals (a live id, an id that names nothing), the
//! opt-in `--clean` strip, and the reopen-only acceptance of that flag. Walks a
//! throwaway git store, since reopen's content lives in history rather than in
//! the tree. Shares the parent module's `flags`/`write`/`TASK` fixtures via
//! [`super`].

use super::*;
use crate::reads::test_support::{git_store, task};

/// A retired ball with the fields a close leaves behind, plus a live claimant.
fn retired(s: &crate::reads::test_support::GitStore) {
    let mut t = task("A retired ball", 100);
    t.claimant = Some("ghost".into());
    t.priority = Some(3);
    t.tags = vec!["bug".into()];
    s.create("bl-1", &t, 100).retire("bl-1", "close", 200);
}

/// `Flags` naming `bl-1`, as the front door parses `bl reopen bl-1`.
fn reopen_flags() -> Flags {
    Flags { positionals: vec!["bl-1".into()], ..flags() }
}

/// The task `reopen` authored, read out of the staged change.
fn staged(store: &Path, f: &Flags) -> Task {
    let d = tempdir().unwrap();
    let (base, before) = base_change(Verb::Reopen, store, f, 900).unwrap();
    // The ball is dead at op start, so a `reopen.pre` plugin sees no prior state.
    assert!(before.is_none(), "a dead ball has no op-start state");
    fs::create_dir_all(d.path().join("tasks")).unwrap();
    base.stage(d.path()).unwrap();
    read_task(d.path(), "bl-1").unwrap()
}

#[test]
fn reopen_authors_the_ball_as_it_stood_before_its_newest_deletion() {
    let s = git_store();
    retired(&s);
    let t = staged(s.dir(), &reopen_flags());
    assert_eq!(t.title, "A retired ball");
    assert_eq!(t.created, 100);
    assert_eq!(t.priority, Some(3));
    assert_eq!(t.tags, ["bug"]);
    // Verbatim by default: the stale claimant comes back untouched.
    assert_eq!(t.claimant.as_deref(), Some("ghost"));
}

#[test]
fn clean_drops_the_claimant_the_close_left_behind() {
    let s = git_store();
    retired(&s);
    let f = Flags { clean: true, ..reopen_flags() };
    let t = staged(s.dir(), &f);
    assert!(t.claimant.is_none(), "--clean restores the ball unclaimed");
    // …and touches nothing else.
    assert_eq!(t.title, "A retired ball");
    assert_eq!(t.priority, Some(3));
}

#[test]
fn reopen_restores_the_newest_incarnation_of_a_reused_id() {
    // An id is a SEQUENCE of incarnations (§ id generation): the recency walk
    // takes the newest deletion, so reopen restores the ball that died LAST.
    let s = git_store();
    s.create("bl-1", &task("first life", 1), 1).retire("bl-1", "close", 2);
    s.create("bl-1", &task("second life", 3), 3).retire("bl-1", "close", 4);
    assert_eq!(staged(s.dir(), &reopen_flags()).title, "second life");
}

#[test]
fn reopen_refuses_a_live_id() {
    // A closed id is legally re-minted, so a live id may be a DIFFERENT ball —
    // restoring over it would clobber a stranger.
    let s = git_store();
    s.create("bl-1", &task("someone else's ball", 1), 1);
    let err = base_change(Verb::Reopen, s.dir(), &reopen_flags(), 900).err().unwrap();
    assert!(err.to_string().contains("bl-1 is live"), "{err}");
}

#[test]
fn reopen_refuses_an_id_that_names_nothing() {
    let s = git_store();
    s.create("bl-other", &task("unrelated", 1), 1);
    let err = base_change(Verb::Reopen, s.dir(), &reopen_flags(), 900).err().unwrap();
    assert!(err.to_string().contains("bl-1 names nothing"), "{err}");
}

#[test]
fn reopen_takes_no_field_edits() {
    let s = git_store();
    retired(&s);
    let f = Flags { title: Some("a new title".into()), ..reopen_flags() };
    let err = base_change(Verb::Reopen, s.dir(), &f, 900).err().unwrap();
    assert!(err.to_string().contains("reopen: takes no field edits"), "{err}");
}

#[test]
fn clean_is_reopen_only() {
    // The flag names reopen's restore mode; every other verb rejects it loudly
    // rather than accepting a word that would do nothing.
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let f = Flags { clean: true, positionals: vec!["bl-1".into()], ..flags() };
    for verb in [Verb::Claim, Verb::Update, Verb::Close] {
        let err = base_change(verb, d.path(), &f, 0).err().unwrap();
        assert!(err.to_string().contains("--clean is reopen-only"), "{verb:?}: {err}");
    }
}
