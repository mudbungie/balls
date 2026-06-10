//! Tests for `bl import`'s stdin grammar — bedrock JSON back to `(id, Task)`.
//! The grammar is exactly what the read verbs emit: one `show --json` object,
//! a `list --json` array, or any concatenation of those; everything else is a
//! named refusal (strict — refuse, don't guess).

use super::*;
use crate::task::On;

/// A minimal fully-identified bedrock record for `id`.
fn record(id: &str) -> String {
    format!(r#"{{"id":"{id}","title":"T","created":5,"updated":9,"body":"b\n"}}"#)
}

#[test]
fn a_single_show_object_parses_verbatim() {
    let got = records(&record("bl-0001")).unwrap();
    let (id, task) = &got[0];
    assert_eq!(id, "bl-0001");
    assert_eq!(task.title, "T");
    // The whole point of import (§16): id and timestamps are taken verbatim —
    // no mint, no `now` stamp.
    assert_eq!((task.created, task.updated), (5, 9));
    assert_eq!(task.body, "b\n");
}

#[test]
fn a_list_array_flattens_and_a_stream_concatenates() {
    // `list --json` (an array) + a trailing `show --json` (an object) in one
    // stream: arrays flatten one level, so a pipe needs no joining filter.
    let text = format!("[{},{}]\n{}\n", record("bl-a"), record("bl-b"), record("bl-c"));
    let ids: Vec<String> = records(&text).unwrap().into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["bl-a", "bl-b", "bl-c"]);
}

#[test]
fn the_canonical_fields_and_extras_round_trip() {
    let text = r#"{
        "id": "bl-full", "title": "Full", "created": 1, "updated": 2,
        "claimant": "ann", "parent": "bl-p", "priority": 3,
        "tags": ["epic"], "blockers": [{"id": "bl-b", "on": "claim"}],
        "state": "in-review", "body": ""
    }"#;
    let (_, task) = records(text).unwrap().pop().unwrap();
    assert_eq!(task.claimant.as_deref(), Some("ann"));
    assert_eq!(task.parent.as_deref(), Some("bl-p"));
    assert_eq!(task.priority, Some(3));
    assert_eq!(task.tags, ["epic"]);
    assert_eq!(task.blockers[0].id, "bl-b");
    assert_eq!(task.blockers[0].on, On::Claim);
    // An unknown key lands in the preserved `extra` table (§3 seam).
    assert_eq!(task.extra["state"].as_str(), Some("in-review"));
}

#[test]
fn a_record_with_null_scalars_parses_as_absent() {
    // The bedrock emits the canonical set TOTAL — a cleared scalar is `null`
    // (§9) — so the inverse must read `null` back as absence.
    let text = r#"{"id":"bl-n","title":"N","created":1,"updated":1,
        "claimant":null,"parent":null,"priority":null,"tags":[],"blockers":[],"body":""}"#;
    let (_, task) = records(text).unwrap().pop().unwrap();
    assert_eq!(task.claimant, None);
    assert_eq!(task.parent, None);
    assert_eq!(task.priority, None);
    // …and the nulls do NOT leak into the preserved extras.
    assert!(task.extra.is_empty());
}

#[test]
fn a_missing_body_defaults_empty_and_a_nonstring_body_is_refused() {
    let (_, task) = records(r#"{"id":"bl-x","title":"T","created":1,"updated":1}"#).unwrap().pop().unwrap();
    assert_eq!(task.body, "");
    let err = records(r#"{"id":"bl-x","title":"T","created":1,"updated":1,"body":3}"#).unwrap_err();
    assert!(err.to_string().contains("\"body\" must be a string"), "{err}");
}

#[test]
fn refusals_name_the_defect() {
    for (text, want) in [
        ("not json", "bad JSON on stdin"),
        ("[3]", "must be a JSON object"),
        (r#"{"title":"no id"}"#, "needs an \"id\""),
        (r#"{"id":"has space","title":"T"}"#, "invalid id"),
        (r#"{"id":7,"title":"T"}"#, "invalid id"),
        // Fully-identified or refused: a record without its timestamps is not
        // a reproduction — `created`/`updated` are required.
        (r#"{"id":"bl-t","title":"T"}"#, "bl-t"),
    ] {
        let err = records(text).unwrap_err();
        assert!(err.to_string().contains(want), "{text} → {err}");
    }
}
