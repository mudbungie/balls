//! §9 `comment` front-door tests (bl-d136) — the body-append authoring (a
//! horizontal-rule seam, nothing else), the empty-body and trailing-newline
//! cases, the empty-TEXT refusal, the positional arity, and the flags the verb
//! does NOT take (`-m` and every field edit). Shares the parent module's
//! `flags`/`write`/`TASK` fixtures via [`super`].

use super::*;
use crate::taskfile::read_task;

/// `bl comment <id> "TEXT"` as [`Flags`].
fn comment(id: &str, text: &str) -> Flags {
    let mut f = flags();
    f.positionals = vec![id.into(), text.into()];
    f
}

#[test]
fn comment_appends_to_the_body_under_a_horizontal_rule() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK); // body: "body\n"
    let (base, before) = base_change(Verb::Comment, dir, &comment("bl-1", "a note"), 5).unwrap();
    assert_eq!(before.unwrap().body, "body\n");
    base.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    // A rule seam, then the literal text — no timestamp, no attribution, no
    // other marker: the commit records who and when (bl-d136). The rule is
    // decoration; nothing ever reads it back.
    assert_eq!(t.body, "body\n\n---\n\na note\n");
    assert_eq!(t.updated, 5);
    // The op seals under its OWN verb, so the §5 trailer and the §6 hook key
    // both read `comment` rather than the `update` it is sugar over.
    assert!(base.finalize(dir).unwrap().contains("bl-op: comment"), "the trailer names the op");
}

#[test]
fn commenting_on_an_empty_body_leaves_no_leading_rule() {
    // A rule separates two things; with nothing above it there is nothing to
    // separate, so the first comment on an empty body is just the text.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", "+++\ntitle = \"A\"\ncreated = 0\nupdated = 0\n+++\n");
    let (base, _) = base_change(Verb::Comment, dir, &comment("bl-1", "first word"), 0).unwrap();
    base.stage(dir).unwrap();
    assert_eq!(read_task(dir, "bl-1").unwrap().body, "first word\n");
}

#[test]
fn successive_comments_each_get_their_own_rule_however_the_body_ended() {
    // A body ending in a RUN of newlines must not widen the seam: both sides are
    // trimmed, so every append is exactly blank line / rule / blank line.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", "+++\ntitle = \"A\"\ncreated = 0\nupdated = 0\n+++\nbody\n\n\n");
    for text in ["first", "second\n\n"] {
        let (base, _) = base_change(Verb::Comment, dir, &comment("bl-1", text), 0).unwrap();
        base.stage(dir).unwrap();
    }
    assert_eq!(read_task(dir, "bl-1").unwrap().body, "body\n\n---\n\nfirst\n\n---\n\nsecond\n");
}

#[test]
fn comment_refuses_empty_or_whitespace_only_text() {
    // A no-op append would seal nothing — the bl-cf93 silent-note-loss failure
    // in a new costume.
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    for text in ["", "   \n\t "] {
        let err = base_change(Verb::Comment, d.path(), &comment("bl-1", text), 0).err().unwrap();
        assert!(err.to_string().contains("TEXT is empty"), "{text:?}: {err}");
    }
}

#[test]
fn comment_needs_exactly_an_id_and_a_text() {
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    for positionals in [vec!["bl-1"], vec![], vec!["bl-1", "a", "b"]] {
        let mut f = flags();
        f.positionals = positionals.iter().map(ToString::to_string).collect();
        let err = base_change(Verb::Comment, d.path(), &f, 0).err().unwrap();
        assert!(err.to_string().contains("expects a task id and the comment TEXT"), "{positionals:?}: {err}");
    }
}

#[test]
fn comment_takes_no_note_and_no_field_edits() {
    // `-m` would store the text twice (the diff already shows it); a field flag
    // or `--edit` would race the append over the payload it IS.
    let d = tempdir().unwrap();
    write(d.path(), "bl-1", TASK);
    let solo: &[fn(&mut Flags)] = &[
        |f| f.message = Some("note".into()),
        |f| f.body = Some("rewritten".into()),
        |f| f.title = Some("t".into()),
        |f| f.edit = true,
    ];
    for (i, set) in solo.iter().enumerate() {
        let mut f = comment("bl-1", "a note");
        set(&mut f);
        let err = base_change(Verb::Comment, d.path(), &f, 0).err().unwrap();
        assert!(err.to_string().contains("takes only <id>"), "flag #{i} slipped through: {err}");
    }
}
