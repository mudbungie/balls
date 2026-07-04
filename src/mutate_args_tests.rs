//! §9 mutating-flag PARSER tests — [`super::parse`] ([`crate::mutate::args`]):
//! every flag and positional, the per-op remote-override ladder, glued short
//! flags, the `--` end-of-options separator, and the missing-value / bad-integer
//! errors. Shares the parent module's `strs` fixture via [`super`]; git-free.

use super::*;

#[test]
fn parse_collects_every_flag_and_positional() {
    let f = parse(
        &strs(&[
            "the-id", "k=v", "--as", "ann", "-m", "note", "--body", "para", "--title", "New",
            "--parent", "bl-p", "--no-parent", "--subtask-of", "bl-e", "--blocks", "bl-g:close",
            "--needs", "bl-n", "--no-needs", "bl-rm", "-p", "3", "--no-priority", "-t", "x",
            "--no-tag", "y", "--edit",
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
    assert_eq!(f.subtask_of.as_deref(), Some("bl-e"));
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
fn parse_resolves_the_per_op_remote_override() {
    // The §12 ladder's top tier on every mutating verb (bl-c2de): `--remote`
    // always assigns, `--center` fills only an empty slot — prime's precedence.
    assert_eq!(parse(&strs(&["--remote", "r"]), "me").unwrap().remote.as_deref(), Some("r"));
    assert_eq!(parse(&strs(&["--center", "c"]), "me").unwrap().remote.as_deref(), Some("c"));
    for order in [["--center", "c", "--remote", "r"], ["--remote", "r", "--center", "c"]] {
        assert_eq!(parse(&strs(&order), "me").unwrap().remote.as_deref(), Some("r"));
    }
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
