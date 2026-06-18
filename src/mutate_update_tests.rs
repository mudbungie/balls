//! §9 `update` front-door dispatch tests — every ball field is overwriteable
//! (title/body/parent/priority/tags/extras and the task's own blockers), the
//! set/clear flag pairs, the create-only `--blocks` + contradiction guards,
//! and the post-hoc live-target refusal (bl-6b8c). Shares the parent module's
//! `flags`/`write`/`TASK` fixtures via [`super`].

use super::*;
use crate::reads::test_support::{git_store, task};
use crate::taskfile::read_task;

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
fn update_refuses_reserved_keys_in_the_extra_seam() {
    // `id` is the filename, `body` is the markdown (--body), created/updated
    // are the seal's stamps — none is a preserved extra. A stored shadow would
    // make the bedrock round-trip lossy (§3/§9), so the seam refuses by name;
    // the clear form (`id=`) is refused identically — there is never a
    // reserved-named extra to remove.
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    for kv in ["id=bl-feed", "body=shadow", "created=999", "updated=999", "id="] {
        let mut f = flags();
        f.positionals = vec!["bl-1".into(), kv.into()];
        let err = base_change(Verb::Update, d.path(), &f, 0).err().unwrap();
        assert!(err.to_string().contains("reserved"), "{kv}: {err}");
    }
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
fn update_rejects_only_blocks_as_create_only() {
    // --blocks (a reciprocal edge on ANOTHER task) stays create-only; --parent is
    // now an ordinary overwriteable field on update.
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    f.blocks = vec!["bl-2:close".into()];
    let err = base_change(Verb::Update, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("create-only"));
}

#[test]
fn update_overwrites_title_body_parent_and_clears_scalars() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", "+++\ntitle = \"Old\"\ncreated = 0\nupdated = 0\nparent = \"bl-p\"\npriority = 5\n+++\nold body\n");
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    f.title = Some("New".into());
    f.body = Some("new body\n".into());
    f.no_parent = true;
    f.no_priority = true;
    let (base, _) = base_change(Verb::Update, dir, &f, 7).unwrap();
    base.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.title, "New");
    assert_eq!(t.body, "new body\n");
    assert!(t.parent.is_none());
    assert!(t.priority.is_none());
}

#[test]
fn update_sets_parent_and_removes_a_tag_and_extra() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", "+++\ntitle = \"A\"\ncreated = 0\nupdated = 0\ntags = [\"a\", \"b\"]\nstate = \"doing\"\n+++\n");
    let mut f = flags();
    f.positionals = vec!["bl-1".into(), "state=".into()]; // empty value removes the extra
    f.parent = Some("bl-new".into());
    f.no_tags = vec!["a".into()];
    let (base, _) = base_change(Verb::Update, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.parent.as_deref(), Some("bl-new"));
    assert_eq!(t.tags, ["b"]);
    assert!(!t.extra.contains_key("state"));
}

#[test]
fn update_rejects_contradictory_set_and_clear() {
    for mut f in [
        {
            let mut f = flags();
            f.parent = Some("bl-2".into());
            f.no_parent = true;
            f
        },
        {
            let mut f = flags();
            f.priority = Some(1);
            f.no_priority = true;
            f
        },
    ] {
        f.positionals = vec!["bl-1".into()];
        let err = base_change(Verb::Update, tempdir().unwrap().path(), &f, 0).err().unwrap();
        assert!(err.to_string().contains("conflict"));
    }
}

#[test]
fn update_rejects_edit_combined_with_field_flags() {
    // --edit and the field-setting flags would race over the payload (§9): a
    // clean either/or, set fields OR hand-edit.
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    f.edit = true;
    f.title = Some("New".into());
    let err = base_change(Verb::Update, d.path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("mutually exclusive"));
}

#[test]
fn update_rejects_edit_combined_with_key_value_extras() {
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into(), "state=doing".into()];
    f.edit = true;
    let err = base_change(Verb::Update, d.path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("the buffer is the payload"));
}

#[test]
fn update_edit_refuses_a_non_tty_so_agents_keep_field_flags() {
    // The detached seam (no tty) is exactly the agent invocation: --edit errors
    // instead of blocking on an editor that can never appear.
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    f.edit = true;
    let err = base_change(Verb::Update, d.path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("not a tty"));
}

#[test]
fn create_rejects_the_update_only_edit_flag() {
    let mut f = flags();
    f.positionals = vec!["a title".into()];
    f.edit = true;
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("--edit is update-only"));
}

#[test]
fn update_adds_and_drops_its_own_blockers() {
    let d = tempdir().unwrap();
    let dir = d.path();
    // bl-1 already carries a claim-blocker on bl-old we will unlink (the §10 in-band fix).
    let before = Task {
        title: "A".into(),
        blockers: vec![Blocker { id: "bl-old".into(), on: On::Claim }],
        ..Task::default()
    };
    write_task(dir, "bl-1", &before).unwrap();
    write(dir, "bl-new", TASK); // an added edge's target must be live (bl-6b8c)
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    f.needs = vec!["bl-new:close".into()]; // add a post-hoc gate
    // Unlink: bare id + tolerant id:op form. Neither target is live — the
    // remove direction is the dangling-edge REMEDY, never refused (bl-6b8c).
    f.no_needs = vec!["bl-old".into(), "bl-z:claim".into()];
    let (base, _) = base_change(Verb::Update, dir, &f, 3).unwrap();
    base.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    // bl-old dropped, bl-new added; the bl-z drop is a harmless no-op.
    assert_eq!(t.blockers, vec![Blocker { id: "bl-new".into(), on: On::Close }]);
    assert_eq!(t.updated, 3);
}

#[test]
fn update_refuses_a_non_live_needs_target() {
    // bl-6b8c: a post-hoc `--needs` validates exactly like create's — refused
    // naming the id and its fate (unknown vs already closed). `--no-needs`
    // and `--edit` stay free: the unlink and the hand-stitch escape hatch.
    let s = git_store();
    s.create("bl-1", &task("A", 1), 1);
    s.create("bl-dead", &task("D", 2), 2).retire("bl-dead", "close", 3);
    for (target, fate) in [("bl-nope", "'bl-nope' is not a known id"), ("bl-dead", "'bl-dead' is already closed")] {
        let mut f = flags();
        f.positionals = vec!["bl-1".into()];
        f.needs = vec![target.into()];
        let err = base_change(Verb::Update, s.dir(), &f, 0).err().unwrap();
        assert!(err.to_string().contains(fate), "{target}: {err}");
    }
}

#[test]
fn update_rejects_subtask_of() {
    // --subtask-of carries a reciprocal claim-gate, so like --blocks it is
    // create-only; update sets --parent (containment) but never a foreign edge.
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let mut f = flags();
    f.positionals = vec!["bl-1".into()];
    f.subtask_of = Some("bl-e".into());
    let err = base_change(Verb::Update, d.path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("create-only"));
}
