//! §3 task tests — parse/round-trip, the unknown-key seam, and the §10 derived
//! predicates (`status`/`ready`/`closeable`), all on in-memory `Task`s with an
//! injected `is_resolved` resolver standing in for the store.

use super::*;

const FULL: &str = concat!(
    "+++\n",
    "title = \"Refactor the foo system\"\n",
    "created = 1748357520\n",
    "updated = 1748443920\n",
    "claimant = \"orionriver@gmail.com\"\n",
    "parent = \"bl-1000\"\n",
    "priority = 2\n",
    "tags = [\"refactor\", \"infra\"]\n",
    "\n",
    "[[blockers]]\n",
    "id = \"bl-1100\"\n",
    "on = \"claim\"\n",
    "\n",
    "[[blockers]]\n",
    "id = \"bl-1200\"\n",
    "on = \"close\"\n",
    "+++\n",
    "Free-form markdown body.\n",
);

fn unresolved(_: &str) -> bool {
    false
}

#[test]
fn parses_every_field_and_the_body() {
    let task = Task::parse(FULL).unwrap();
    assert_eq!(task.title, "Refactor the foo system");
    assert_eq!(task.created, 1_748_357_520);
    assert_eq!(task.updated, 1_748_443_920);
    assert_eq!(task.claimant.as_deref(), Some("orionriver@gmail.com"));
    assert_eq!(task.parent.as_deref(), Some("bl-1000"));
    assert_eq!(task.priority, Some(2));
    assert_eq!(
        task.blockers,
        vec![
            Blocker { id: "bl-1100".into(), on: On::Claim },
            Blocker { id: "bl-1200".into(), on: On::Close },
        ]
    );
    assert_eq!(task.tags, ["refactor", "infra"]);
    assert_eq!(task.body, "Free-form markdown body.\n");
}

#[test]
fn full_task_round_trips_byte_for_byte() {
    assert_eq!(Task::parse(FULL).unwrap().to_markdown(), FULL);
}

#[test]
fn a_minimal_task_omits_optionals_and_has_no_body() {
    let src = "+++\ntitle = \"t\"\ncreated = 1\nupdated = 2\n+++\n";
    let task = Task::parse(src).unwrap();
    assert_eq!(task.claimant, None);
    assert!(task.blockers.is_empty());
    assert_eq!(task.body, "");
    assert_eq!(task.to_markdown(), src);
}

#[test]
fn unknown_keys_survive_a_round_trip() {
    let src = "+++\ntitle = \"t\"\ncreated = 1\nupdated = 2\nstate = \"doing\"\n+++\nbody\n";
    let task = Task::parse(src).unwrap();
    assert_eq!(
        task.extra.get("state").and_then(toml::Value::as_str),
        Some("doing")
    );
    assert_eq!(task.to_markdown(), src);
}

#[test]
fn missing_opening_fence_is_an_error() {
    let err = Task::parse("title = \"t\"\n").unwrap_err();
    assert!(matches!(err, ParseError::MissingFrontmatter));
    assert!(err.to_string().contains("missing opening"));
}

#[test]
fn missing_closing_fence_is_an_error() {
    let err = Task::parse("+++\ntitle = \"t\"\n").unwrap_err();
    assert!(matches!(err, ParseError::UnterminatedFrontmatter));
    assert!(err.to_string().contains("unterminated"));
}

#[test]
fn invalid_toml_frontmatter_is_an_error() {
    let err = Task::parse("+++\ntitle = [unterminated\n+++\n").unwrap_err();
    assert!(matches!(err, ParseError::Toml(_)));
    assert!(err.to_string().contains("invalid frontmatter"));
}

#[test]
fn a_shadow_id_or_body_frontmatter_key_is_an_error() {
    // The id is the filename and the body is the markdown after the fence —
    // a stored `id =`/`body =` line would be a shadow the bedrock projection
    // silently drops (§3/§9), so the schema refuses to parse it at all.
    for key in ["id", "body"] {
        let src = format!("+++\ntitle = \"t\"\ncreated = 1\nupdated = 2\n{key} = \"shadow\"\n+++\n");
        let err = Task::parse(&src).unwrap_err();
        assert!(matches!(err, ParseError::ReservedKey(k) if k == key));
        assert!(err.to_string().contains(&format!("'{key}'")), "{err}");
        assert!(err.to_string().contains("filename"), "{err}");
    }
}

#[test]
fn a_claimant_yields_claimed_even_with_an_open_blocker() {
    let mut task = Task::parse(FULL).unwrap();
    task.claimant = Some("me".into());
    assert_eq!(task.status(&unresolved), Status::Claimed);
    assert!(!task.ready(&unresolved));
}

#[test]
fn an_unresolved_claim_blocker_yields_blocked() {
    let mut task = Task::parse(FULL).unwrap();
    task.claimant = None;
    assert_eq!(task.status(&unresolved), Status::Blocked);
    assert!(!task.ready(&unresolved));
}

#[test]
fn a_resolved_claim_blocker_yields_ready() {
    let mut task = Task::parse(FULL).unwrap();
    task.claimant = None;
    assert_eq!(task.status(&|_| true), Status::Ready);
    assert!(task.ready(&|_| true));
}

#[test]
fn a_lone_close_blocker_never_blocks_status() {
    let mut task = Task::parse(FULL).unwrap();
    task.claimant = None;
    task.blockers.retain(|b| b.on == On::Close);
    assert_eq!(task.status(&unresolved), Status::Ready);
}

#[test]
fn closeable_only_when_every_close_blocker_is_resolved() {
    // FULL carries one close-blocker (bl-1200) and one claim-blocker (bl-1100).
    let task = Task::parse(FULL).unwrap();
    assert!(!task.closeable(&unresolved)); // bl-1200 still open
    assert!(task.closeable(&|id| id == "bl-1200")); // resolve only the gate
}

#[test]
fn a_task_with_no_close_blocker_is_always_closeable() {
    let mut task = Task::parse(FULL).unwrap();
    task.blockers.retain(|b| b.on == On::Claim);
    assert!(task.closeable(&unresolved));
}
