//! Tests for the `TaskType` newtype — strict parse, lenient
//! deserialize, epic predicate, and round-trip shape.

use super::*;

#[test]
fn parse_accepts_any_identifier() {
    for s in ["epic", "task", "bug", "feature", "chore", "spike", "retro-2", "a"] {
        assert_eq!(TaskType::parse(s).unwrap().as_str(), s, "{s}");
    }
}

#[test]
fn parse_rejects_non_identifiers() {
    // Empty, uppercase, leading digit, leading punct, whitespace —
    // every non-identifier must fail up front so `bl create -t`
    // cannot smuggle a weird value onto disk.
    for s in ["", "Spike", "1bug", "-feature", "fix thing", "feat/x", "feat."] {
        assert!(TaskType::parse(s).is_err(), "should reject: {s:?}");
    }
}

#[test]
fn deserialize_is_lenient() {
    // Forward-compat: a newer bl may write a label that `parse`
    // rejects (e.g. spaces). Old bl must round-trip it instead of
    // bricking on load.
    let back: TaskType = serde_json::from_str("\"Spike With Space\"").unwrap();
    assert_eq!(back.as_str(), "Spike With Space");
    let s = serde_json::to_string(&back).unwrap();
    assert_eq!(s, "\"Spike With Space\"");
}

#[test]
fn round_trip_preserves_value() {
    for s in ["epic", "task", "bug", "feature", "spike"] {
        let tt = TaskType::parse(s).unwrap();
        let j = serde_json::to_string(&tt).unwrap();
        let back: TaskType = serde_json::from_str(&j).unwrap();
        assert_eq!(back, tt);
    }
}

#[test]
fn is_epic_only_for_epic_string() {
    assert!(TaskType::epic().is_epic());
    assert!(TaskType::parse("epic").unwrap().is_epic());
    assert!(!TaskType::task().is_epic());
    assert!(!TaskType::bug().is_epic());
    assert!(!TaskType::parse("feature").unwrap().is_epic());
}

#[test]
fn as_str_and_display() {
    let tt = TaskType::parse("spike").unwrap();
    assert_eq!(tt.as_str(), "spike");
    assert_eq!(format!("{tt}"), "spike");
}

#[test]
fn suggestions_are_all_valid_identifiers() {
    for s in TaskType::SUGGESTIONS {
        assert!(TaskType::parse(s).is_ok(), "suggestion must parse: {s}");
    }
}
