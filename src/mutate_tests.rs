//! §9 mutating-dispatch tests — the parser, the per-verb [`base_change`]
//! authoring, and the front-door guards, exercised on a plain temp dir (the
//! authoring is git-free, so no terminus is needed). The full engine seal is
//! covered end to end by the `lib`/`dispatch` integration tests.

use super::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

const TASK: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\n+++\nbody\n";

/// A `Flags` with `actor` set and everything else empty.
fn flags() -> Flags {
    Flags { actor: "me".into(), ..Flags::default() }
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
            "the-id", "k=v", "--as", "ann", "-m", "subj", "--body", "para", "--parent", "bl-p",
            "--gates", "bl-g", "--needs", "bl-n", "-p", "3", "-t", "x",
        ]),
        "default",
    )
    .unwrap();
    assert_eq!(f.actor, "ann");
    assert_eq!(f.over.as_deref(), Some("subj"));
    assert_eq!(f.body.as_deref(), Some("para"));
    assert_eq!(f.parent.as_deref(), Some("bl-p"));
    assert_eq!(f.gates.as_deref(), Some("bl-g"));
    assert_eq!(f.needs, ["bl-n"]);
    assert_eq!(f.priority, Some(3));
    assert_eq!(f.tags, ["x"]);
    assert_eq!(f.positionals, ["the-id", "k=v"]);
    // The default actor applies when --as is absent.
    assert_eq!(parse(&[], "default").unwrap().actor, "default");
}

#[test]
fn parse_rejects_an_unknown_flag() {
    assert!(parse(&strs(&["--nope"]), "me").is_err());
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
fn create_authors_a_ball_with_its_front_door_structure() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let mut f = flags();
    f.positionals = vec!["New thing".into()];
    f.parent = Some("bl-1000".into());
    f.needs = vec!["bl-9".into()];
    f.priority = Some(2);
    f.tags = vec!["x".into()];
    let (base, before) = base_change(Verb::Create, dir, &f, 42).unwrap();
    assert!(before.is_none()); // create has no op-start ball
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000"]);
    let t = read_task(dir, &id).unwrap();
    assert_eq!(t.title, "New thing");
    assert_eq!(t.created, 42);
    assert_eq!(t.priority, Some(2));
    assert_eq!(t.tags, ["x"]);
    assert_eq!(t.blockers, vec![Blocker { id: "bl-9".into(), on: On::Claim }]);
    // --parent writes the reciprocal claim-blocker on the epic (§10).
    let parent = read_task(dir, "bl-1000").unwrap();
    assert_eq!(parent.blockers, vec![Blocker { id, on: On::Claim }]);
}

#[test]
fn create_with_gates_writes_a_close_reciprocal() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let mut f = flags();
    f.positionals = vec!["Gate".into()];
    f.gates = Some("bl-1000".into());
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000"]);
    assert_eq!(read_task(dir, "bl-1000").unwrap().blockers, vec![Blocker { id, on: On::Close }]);
}

#[test]
fn create_rejects_parent_and_gates_together() {
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.parent = Some("a".into());
    f.gates = Some("b".into());
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("mutually exclusive"));
}

#[test]
fn create_requires_exactly_one_positional() {
    let dir = tempdir().unwrap();
    assert!(base_change(Verb::Create, dir.path(), &flags(), 0).is_err()); // zero
    let mut f = flags();
    f.positionals = vec!["a".into(), "b".into()];
    assert!(base_change(Verb::Create, dir.path(), &f, 0).is_err()); // two
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
}

#[test]
fn update_builds_extras_priority_and_tags() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into(), "state=doing".into()];
    f.priority = Some(5);
    f.tags = vec!["urgent".into()];
    let (base, before) = base_change(Verb::Update, dir, &f, 9).unwrap();
    assert_eq!(before.unwrap().title, "A task");
    base.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.extra.get("state").and_then(toml::Value::as_str), Some("doing"));
    assert_eq!(t.priority, Some(5));
    assert_eq!(t.tags, ["urgent"]);
    assert_eq!(t.updated, 9);
}

#[test]
fn update_requires_a_task_id() {
    let err = base_change(Verb::Update, tempdir().unwrap().path(), &flags(), 0).err().unwrap();
    assert!(err.to_string().contains("needs a task id"));
}

#[test]
fn update_rejects_a_non_key_value_positional() {
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into(), "notpair".into()];
    let err = base_change(Verb::Update, d.path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("not key=value"));
}

#[test]
fn update_rejects_structural_flags() {
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    f.needs = vec!["bl-2".into()];
    let err = base_change(Verb::Update, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("only for create"));
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
    assert!(c.field_changes.is_empty());
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
