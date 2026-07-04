//! §9 mutating-dispatch tests — the parser, the per-verb [`base_change`]
//! authoring, and the front-door guards, exercised on a plain temp dir (the
//! authoring is git-free, so no anvil is needed — except the bl-6b8c
//! dead-vs-unknown refusals, which walk history on a throwaway git store).
//! The full engine seal is covered end to end by the `lib`/`dispatch`
//! integration tests.

use super::*;
use crate::task::{Blocker, On};
use crate::taskfile::{read_task, task_ids, write_task};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

const TASK: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\n+++\nbody\n";

/// A `Flags` with `actor` set and everything else empty.
fn flags() -> Flags {
    Flags { actor: "me".into(), ..Flags::default() }
}

/// [`super::base_change`] with a detached editor seam — the flag-driven paths,
/// which never no-op. Shadows the real fn so the per-verb tests stay
/// signature-stable; the `--edit` interaction is exercised in
/// [`crate::mutate::edit`]'s own tests.
fn base_change(verb: Verb, store: &Path, flags: &Flags, now: i64) -> io::Result<super::author::Authored> {
    super::base_change(verb, store, flags, now, Vec::new(), &mut edit::Editor::detached())
        .map(|authored| authored.expect("flag-driven authoring never no-ops"))
}

/// Write `tasks/<id>.md` under `dir`.
fn write(dir: &Path, id: &str, md: &str) {
    let tasks = dir.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join(format!("{id}.md")), md).unwrap();
}

/// The single ball id under `dir/tasks` not in `known` (the just-minted one).
/// `known` MUST list EVERY pre-existing id, else an arbitrary leftover is
/// returned — a filesystem-order wrong-ball flake (bl-8c74).
fn new_id(dir: &Path, known: &[&str]) -> String {
    task_ids(dir).unwrap().into_iter().find(|id| !known.contains(&id.as_str())).unwrap()
}

fn strs(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

#[test]
fn claim_authors_occupancy_and_returns_the_before_state() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    let (base, before) = base_change(Verb::Claim, dir, &f, 7).unwrap();
    assert_eq!(before.unwrap().title, "A task");
    base.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.claimant.as_deref(), Some("me"));
    assert_eq!(t.updated, 7);
}

#[test]
fn unclaim_clears_the_claimant() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", "+++\ntitle = \"A\"\ncreated = 0\nupdated = 0\nclaimant = \"bob\"\n+++\n");
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    let (base, _) = base_change(Verb::Unclaim, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    assert!(read_task(dir, "bl-1").unwrap().claimant.is_none());
}

#[test]
fn an_occupancy_verb_rejects_shaping_flags() {
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let id = || {
        let mut f = flags();
        f.positionals = vec!["bl-1".into()];
        f
    };
    // structure (--parent) and shaping (-p) both bounce.
    let mut structural = id();
    structural.parent = Some("bl-2".into());
    assert!(base_change(Verb::Claim, d.path(), &structural, 0).is_err());
    let mut shaping = id();
    shaping.priority = Some(1);
    assert!(base_change(Verb::Close, d.path(), &shaping, 0).is_err());
    // --no-tag is the last predicate in the chain — setting only it forces the
    // whole guard to evaluate, and it still bounces (no field edits on retire).
    let mut last = id();
    last.no_tags = vec!["x".into()];
    let err = base_change(Verb::Unclaim, d.path(), &last, 0).err().unwrap();
    assert!(err.to_string().contains("no field edits"));
    // -m (commit narration) and --as are the only flags an occupancy verb takes.
    let mut narrated = id();
    narrated.message = Some("note".into());
    assert!(base_change(Verb::Claim, d.path(), &narrated, 0).is_ok());
    // --edit (the whole-buffer shape) bounces like any field edit.
    let mut edited = id();
    edited.edit = true;
    let err = base_change(Verb::Claim, d.path(), &edited, 0).err().unwrap();
    assert!(err.to_string().contains("no field edits"));
}

#[test]
fn each_shaping_flag_bounces_an_occupancy_verb_on_its_own() {
    // shapes() is a pure disjunction: EVERY field flag must trip the guard
    // ALONE (an && slipped into the chain would let a lone flag through).
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let solo: &[fn(&mut Flags)] = &[
        |f| f.title = Some("t".into()),
        |f| f.body = Some("b".into()),
        |f| f.parent = Some("bl-p".into()),
        |f| f.no_parent = true,
        |f| f.subtask_of = Some("bl-e".into()),
        |f| f.no_priority = true,
        |f| f.priority = Some(1),
        |f| f.blocks = vec!["close".into()],
        |f| f.needs = vec!["bl-n".into()],
        |f| f.no_needs = vec!["bl-n".into()],
        |f| f.tags = vec!["x".into()],
        |f| f.no_tags = vec!["x".into()],
    ];
    for (i, set) in solo.iter().enumerate() {
        let mut f = flags();
        f.positionals = vec!["bl-1".into()];
        set(&mut f);
        assert!(base_change(Verb::Claim, d.path(), &f, 0).is_err(), "solo flag #{i} slipped through");
    }
}

#[test]
fn create_rejects_title_flag_and_uses_the_positional() {
    let mut f = flags();
    f.positionals = vec!["the title".into()];
    f.title = Some("via flag".into());
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("positional argument, not --title"));
}

#[test]
fn close_retires_the_ball() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    let (base, before) = base_change(Verb::Close, dir, &f, 0).unwrap();
    assert_eq!(before.unwrap().title, "A task");
    base.stage(dir).unwrap();
    assert!(!dir.join("tasks/bl-1.md").exists());
    // finalize still renders the captured title once the file is gone.
    assert!(base.finalize(dir).unwrap().starts_with("A task"));
}

#[test]
fn base_change_rejects_a_non_mutating_verb() {
    let err = base_change(Verb::Show, tempdir().unwrap().path(), &flags(), 0).err().unwrap();
    assert!(err.to_string().contains("not a mutating verb"));
}

#[test]
fn command_marks_a_mutating_op_and_carries_the_body() {
    let mut f = flags();
    f.body = Some("para".into());
    let c = command(Verb::Create, &f);
    assert_eq!(c.op, "create");
    assert_eq!(c.body_change.as_deref(), Some("para"));
}

#[test]
fn now_reads_a_positive_clock() {
    assert!(now() > 1_700_000_000); // sometime after 2023
}

#[test]
fn children_notice_agrees_in_number_and_stays_silent_at_zero() {
    // bl-3ddb: "closed with 1 open children" was ungrammatical; one survivor
    // reads singular, several keep the plural, none emits nothing.
    assert_eq!(super::report::children_notice("bl-x", 0), None);
    assert_eq!(
        super::report::children_notice("bl-x", 1).unwrap(),
        "notice: bl-x closed with 1 open child, not gating — its parent pointer now dangles (display-only)"
    );
    assert_eq!(
        super::report::children_notice("bl-x", 2).unwrap(),
        "notice: bl-x closed with 2 open children, none gating — their parent pointers now dangle (display-only)"
    );
}

#[test]
fn change_tokens_are_hex_and_distinct() {
    let (a, b) = (change_token(), change_token());
    assert_eq!(a.len(), 32);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

// The flag PARSER tests share this module's `strs` fixture.
#[path = "mutate_args_tests.rs"]
mod args;

// The `create` front-door tests share this module's flags/write/new_id fixtures.
#[path = "mutate_create_tests.rs"]
mod create;

// The `update` front-door tests share this module's flags/write/TASK fixtures.
#[path = "mutate_update_tests.rs"]
mod update;
