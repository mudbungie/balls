use super::*;
use crate::negotiation::CommitPolicy;
use crate::participant::{Event, Field};

#[test]
fn describe_round_trips_minimal_shape() {
    let json = r#"{
        "subscriptions": ["claim", "review"],
        "projection": { "external_prefixes": ["jira"] }
    }"#;
    let resp: DescribeResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.subscriptions, vec![Event::Claim, Event::Review]);
    assert_eq!(resp.projection.external_prefixes, vec!["jira".to_string()]);
    assert!(resp.retry_budget.is_none());
}

#[test]
fn describe_accepts_full_projection_and_retry_budget() {
    let json = r#"{
        "subscriptions": ["close"],
        "projection": {
            "owns": ["status", "external"],
            "reads": ["title"],
            "external_prefixes": ["jira"]
        },
        "retry_budget": 7
    }"#;
    let resp: DescribeResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.retry_budget, Some(7));
    let proj = resp.projection.into_projection().unwrap();
    assert!(proj.owns.contains(&Field::Status));
    assert!(proj.owns.contains(&Field::External));
    assert!(proj.reads.contains(&Field::Title));
    assert!(proj.external_prefixes.contains("jira"));
}

#[test]
fn field_wire_name_round_trips_with_parse_field() {
    // Walking every variant guarantees a future Field addition fails
    // here until parse + wire_name are extended in lockstep.
    for f in Field::all() {
        let name = field_wire_name(f);
        assert_eq!(parse_field(name).unwrap(), f);
    }
}

#[test]
fn parse_field_covers_every_canonical_variant() {
    // Exhaustively walk every `Field` variant so a future addition to
    // the enum breaks this test (and reminds the author to extend the
    // parser rather than silently dropping the field on the wire).
    for f in Field::all() {
        let name = match f {
            Field::Title => "title",
            Field::Type => "type",
            Field::Priority => "priority",
            Field::Status => "status",
            Field::Parent => "parent",
            Field::DependsOn => "depends_on",
            Field::Description => "description",
            Field::Tags => "tags",
            Field::Notes => "notes",
            Field::Links => "links",
            Field::ClaimedBy => "claimed_by",
            Field::Branch => "branch",
            Field::ClosedAt => "closed_at",
            Field::UpdatedAt => "updated_at",
            Field::ClosedChildren => "closed_children",
            Field::External => "external",
            Field::SyncedAt => "synced_at",
            Field::DeliveredIn => "delivered_in",
        };
        assert_eq!(parse_field(name).unwrap(), f);
    }
}

#[test]
fn parse_field_rejects_unknown_name() {
    let err = parse_field("frobnicator").unwrap_err();
    assert!(format!("{err}").contains("unknown projection field"));
}

#[test]
fn projection_wire_propagates_field_parse_error_in_owns() {
    let wire = ProjectionWire {
        owns: vec!["bogus".into()],
        ..ProjectionWire::default()
    };
    let err = wire.into_projection().unwrap_err();
    assert!(format!("{err}").contains("bogus"));
}

#[test]
fn projection_wire_propagates_field_parse_error_in_reads() {
    let wire = ProjectionWire {
        reads: vec!["nonexistent".into()],
        ..ProjectionWire::default()
    };
    let err = wire.into_projection().unwrap_err();
    assert!(format!("{err}").contains("nonexistent"));
}

#[test]
fn propose_response_can_be_ok_only() {
    let json = r#"{ "ok": { "task": {"external": {"jira": {"id": "J-1"}}} } }"#;
    let resp: ProposeResponse = serde_json::from_str(json).unwrap();
    assert!(resp.ok.is_some());
    assert!(resp.conflict.is_none());
}

#[test]
fn propose_response_can_be_conflict_only() {
    let json = r#"{ "conflict": { "fields": ["status"], "remote_view": {"status": "review"}, "hint": "advanced" } }"#;
    let resp: ProposeResponse = serde_json::from_str(json).unwrap();
    let c = resp.conflict.unwrap();
    assert_eq!(c.fields, vec!["status".to_string()]);
    assert_eq!(c.hint.as_deref(), Some("advanced"));
}

#[test]
fn propose_response_neither_branch_means_neither_set() {
    let resp: ProposeResponse = serde_json::from_str("{}").unwrap();
    assert!(resp.ok.is_none());
    assert!(resp.conflict.is_none());
}

#[test]
fn commit_policy_wire_round_trips_each_variant() {
    let cases = [
        (
            r#"{"kind":"commit","message":"hello"}"#,
            CommitPolicy::Commit { message: Some("hello".into()) },
        ),
        (r#"{"kind":"commit"}"#, CommitPolicy::Commit { message: None }),
        (
            r#"{"kind":"batch","tag":"audit"}"#,
            CommitPolicy::Batch { tag: "audit".into() },
        ),
        (r#"{"kind":"suppress"}"#, CommitPolicy::Suppress),
    ];
    for (json, expected) in cases {
        let wire: CommitPolicyWire = serde_json::from_str(json).unwrap();
        assert_eq!(wire.into_policy(), expected);
    }
}
