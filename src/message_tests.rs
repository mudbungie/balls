use super::*;

fn task_msg() -> Message {
    Message {
        verb: Verb::Close,
        actor: "me@example.com".into(),
        id: Some("bl-1234".into()),
        subject: "Refactor the foo system".into(),
        body: Some("A free-form paragraph.".into()),
    }
}

#[test]
fn render_emits_subject_body_and_the_core_trailers() {
    let text = task_msg().render().unwrap();
    assert!(text.starts_with("Refactor the foo system\n\n"));
    assert!(text.contains("A free-form paragraph."));
    let md = parse(&text).unwrap();
    assert_eq!(md["bl-protocol"], ["1"]);
    assert_eq!(md["bl-op"], ["close"]);
    assert_eq!(md["bl-id"], ["bl-1234"]);
    assert_eq!(md["bl-actor"], ["me@example.com"]);
}

#[test]
fn a_body_without_a_trailing_newline_still_seals_a_parseable_bl_id() {
    // bl-5066: `render` must hand `interpret-trailers` a newline-terminated
    // body. Without it, old git (≤2.43) fuses the appended trailer block onto
    // the last body paragraph — no blank-line separator — so `--parse` finds no
    // block and the sealed `bl-id` is lost, panicking `sealed_id`. This body
    // ends WITHOUT a newline, the exact shape that regressed; the fix keeps the
    // trailer block its own blank-line-separated paragraph on every git version.
    let msg = Message {
        body: Some("Final line, no newline".into()),
        ..task_msg()
    };
    let text = msg.render().unwrap();
    assert!(text.contains("Final line, no newline\n\nbl-protocol: 1"));
    assert_eq!(parse(&text).unwrap()["bl-id"], ["bl-1234"]);
}

#[test]
fn a_body_that_already_ends_in_a_newline_seals_one_clean_trailer_block() {
    // The `render` newline guard is conditional (append only if absent), so a
    // body that already ends in a newline round-trips without a doubled blank
    // line collapsing the block (bl-5066).
    let msg = Message {
        body: Some("Trailing newline body\n".into()),
        ..task_msg()
    };
    let md = parse(&msg.render().unwrap()).unwrap();
    assert_eq!(md["bl-id"], ["bl-1234"]);
    assert_eq!(md["bl-op"], ["close"]);
}

#[test]
fn core_trailers_render_in_protocol_op_id_actor_order() {
    let text = task_msg().render().unwrap();
    let pos = |k: &str| text.find(k).unwrap();
    assert!(pos("bl-protocol") < pos("bl-op"));
    assert!(pos("bl-op") < pos("bl-id"));
    assert!(pos("bl-id") < pos("bl-actor"));
}

#[test]
fn a_bodyless_message_is_subject_then_trailers() {
    let msg = Message {
        body: None,
        subject: "Just a subject".into(),
        ..task_msg()
    };
    let text = msg.render().unwrap();
    assert_eq!(text, "Just a subject\n\nbl-protocol: 1\nbl-op: close\nbl-id: bl-1234\nbl-actor: me@example.com\n");
}

#[test]
fn a_checkout_op_names_no_ball_so_omits_bl_id() {
    let msg = Message {
        verb: Verb::Install,
        id: None,
        ..task_msg()
    };
    let md = parse(&msg.render().unwrap()).unwrap();
    assert_eq!(md["bl-op"], ["install"]);
    assert!(!md.contains_key("bl-id"));
}

#[test]
fn a_plugin_trailer_in_the_body_is_preserved_alongside_core_keys() {
    let msg = Message {
        body: Some("Fixes the thing.\n\njira-id: ABC-1".into()),
        ..task_msg()
    };
    let md = parse(&msg.render().unwrap()).unwrap();
    assert_eq!(md["jira-id"], ["ABC-1"]);
    assert_eq!(md["bl-op"], ["close"]);
}

#[test]
fn parse_groups_a_repeated_key_into_a_value_list() {
    let md = parse("Subject\n\nbl-tag: a\nbl-tag: b\nbl-op: update\n").unwrap();
    assert_eq!(md["bl-tag"], ["a", "b"]);
    assert_eq!(md["bl-op"], ["update"]);
}

#[test]
fn a_git_that_exits_non_zero_is_an_error_not_an_empty_parse() {
    // bl-dede: this spawn used to return `Ok(stdout)` whatever git did, so a
    // failed child read downstream as "no trailers here" and the §9 report
    // panicked on the missing `bl-id` — after the op was durable. The exit
    // status is checked at the spawn, where the failure actually is.
    let err = run_git(&["not-a-git-subcommand"], "Subject\n").unwrap_err();
    assert!(err.to_string().starts_with("git not-a-git-subcommand:"), "{err}");
}

#[test]
fn parse_of_a_trailerless_message_is_empty() {
    assert!(parse("Subject only\n\nA body with no trailer block.\n")
        .unwrap()
        .is_empty());
}

#[test]
fn a_git_that_dies_before_draining_stdin_still_reports_git_not_the_broken_pipe() {
    // bl-2695: the same failure as above, made DETERMINISTIC. A payload past
    // the 64 KiB pipe buffer cannot be parked in the kernel and walked away
    // from — the write blocks until the dead child's closed read end turns it
    // into EPIPE, every run. Propagating that error masked the exit status and
    // handed the operator `Broken pipe (os error 32)`, naming neither the git
    // command nor the reason; the flake surfaced it one time in a hundred (a
    // close gate under load), which is the worst way to learn it.
    let payload = "Subject\n\n".to_string() + &"a filler line to outgrow the pipe buffer\n".repeat(4096);
    assert!(payload.len() > 64 * 1024, "must exceed the pipe buffer: {}", payload.len());
    let err = run_git(&["not-a-git-subcommand"], &payload).unwrap_err();
    assert!(err.to_string().starts_with("git not-a-git-subcommand:"), "{err}");
}
