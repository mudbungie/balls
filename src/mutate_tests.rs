//! §9 mutating-dispatch tests — the parser, the per-verb [`base_change`]
//! authoring, and the front-door guards, exercised on a plain temp dir (the
//! authoring is git-free, so no anvil is needed). The full engine seal is
//! covered end to end by the `lib`/`dispatch` integration tests.

use super::*;
use crate::task::{Blocker, On};
use crate::taskfile::write_task;
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
fn base_change(verb: Verb, store: &Path, flags: &Flags, now: i64) -> io::Result<Authored> {
    super::base_change(verb, store, flags, now, &mut edit::Editor::detached())
        .map(|authored| authored.expect("flag-driven authoring never no-ops"))
}

/// Write `tasks/<id>.md` under `dir`.
fn write(dir: &Path, id: &str, md: &str) {
    let tasks = dir.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join(format!("{id}.md")), md).unwrap();
}

/// The single ball id under `dir/tasks` that is not in `known`.
fn new_id(dir: &Path, known: &[&str]) -> String {
    task_ids(dir).unwrap().into_iter().find(|id| !known.contains(&id.as_str())).unwrap()
}

fn strs(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

#[test]
fn parse_collects_every_flag_and_positional() {
    let f = parse(
        &strs(&[
            "the-id", "k=v", "--as", "ann", "-m", "note", "--body", "para", "--title", "New",
            "--parent", "bl-p", "--no-parent", "--blocks", "bl-g:close", "--needs", "bl-n",
            "--no-needs", "bl-rm", "-p", "3", "--no-priority", "-t", "x", "--no-tag", "y",
            "--edit",
        ]),
        "default",
    )
    .unwrap();
    assert_eq!(f.actor, "ann");
    assert_eq!(f.message.as_deref(), Some("note"));
    assert_eq!(f.body.as_deref(), Some("para"));
    assert_eq!(f.title.as_deref(), Some("New"));
    assert_eq!(f.parent.as_deref(), Some("bl-p"));
    assert!(f.no_parent);
    assert_eq!(f.blocks, ["bl-g:close"]);
    assert_eq!(f.needs, ["bl-n"]);
    assert_eq!(f.no_needs, ["bl-rm"]);
    assert_eq!(f.priority, Some(3));
    assert!(f.no_priority);
    assert_eq!(f.tags, ["x"]);
    assert_eq!(f.no_tags, ["y"]);
    assert!(f.edit);
    assert_eq!(f.positionals, ["the-id", "k=v"]);
    // The default actor applies when --as is absent.
    assert_eq!(parse(&[], "default").unwrap().actor, "default");
}

#[test]
fn parse_rejects_an_unknown_flag() {
    assert!(parse(&strs(&["--nope"]), "me").is_err());
}

#[test]
fn parse_accepts_glued_short_flags() {
    // -p1 == -p 1 (the getopt convention); -t and -m glue the same way.
    let f = parse(&strs(&["a title", "-p1", "-turgent", "-mglued note"]), "me").unwrap();
    assert_eq!(f.priority, Some(1));
    assert_eq!(f.tags, ["urgent"]);
    assert_eq!(f.message.as_deref(), Some("glued note"));
    assert_eq!(f.positionals, ["a title"]);
    // A glued negative priority splits cleanly too (-p-5 → -p -5).
    assert_eq!(parse(&strs(&["-p-5"]), "me").unwrap().priority, Some(-5));
    // The split form is untouched, and an unknown short glue still bounces.
    assert_eq!(parse(&strs(&["-p", "2"]), "me").unwrap().priority, Some(2));
    assert!(parse(&strs(&["-x1"]), "me").is_err());
}

#[test]
fn parse_honors_the_end_of_options_separator() {
    // Everything after `--` is a positional, however `-`-leading — the seam a
    // caller shelling an untrusted title uses (`bl create -- "$TITLE"`).
    let f = parse(&strs(&["-p", "1", "--", "--title", "-p2", "--"]), "me").unwrap();
    assert_eq!(f.priority, Some(1));
    assert!(f.title.is_none());
    assert_eq!(f.positionals, ["--title", "-p2", "--"]);
    // Gluing stops at the separator too: a `-p1` title survives whole.
    assert_eq!(parse(&strs(&["--", "-p1"]), "me").unwrap().positionals, ["-p1"]);
    // A trailing bare `--` adds nothing.
    assert!(parse(&strs(&["--"]), "me").unwrap().positionals.is_empty());
}

#[test]
fn parse_errors_on_a_flag_missing_its_value() {
    let err = parse(&strs(&["--as"]), "me").unwrap_err();
    assert!(err.to_string().contains("--as needs a value"));
}

#[test]
fn parse_rejects_a_non_integer_priority() {
    let err = parse(&strs(&["-p", "high"]), "me").unwrap_err();
    assert!(err.to_string().contains("not an integer"));
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
    let err = base_change(Verb::Drop, d.path(), &last, 0).err().unwrap();
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
fn create_rejects_title_flag_and_uses_the_positional() {
    let mut f = flags();
    f.positionals = vec!["the title".into()];
    f.title = Some("via flag".into());
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("positional argument, not --title"));
}

#[test]
fn close_and_drop_retire_the_ball() {
    for verb in [Verb::Close, Verb::Drop] {
        let d = tempdir().unwrap();
        let dir = d.path();
        write(dir, "bl-1", TASK);
        let mut f = flags();
        f.positionals = vec!["bl-1".into()];
        let (base, before) = base_change(verb, dir, &f, 0).unwrap();
        assert_eq!(before.unwrap().title, "A task");
        base.stage(dir).unwrap();
        assert!(!dir.join("tasks/bl-1.md").exists());
        // finalize still renders the captured title once the file is gone.
        assert!(base.finalize(dir).unwrap().starts_with("A task"));
    }
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
fn change_tokens_are_hex_and_distinct() {
    let (a, b) = (change_token(), change_token());
    assert_eq!(a.len(), 32);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

// The `create` front-door tests share this module's flags/write/new_id fixtures.
#[path = "mutate_create_tests.rs"]
mod create;

// The `update` front-door tests share this module's flags/write/TASK fixtures.
#[path = "mutate_update_tests.rs"]
mod update;
