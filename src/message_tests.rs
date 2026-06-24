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
fn parse_of_a_trailerless_message_is_empty() {
    assert!(parse("Subject only\n\nA body with no trailer block.\n")
        .unwrap()
        .is_empty());
}
