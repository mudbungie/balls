//! Typed relationships between tasks: `Link` and `LinkType`.
//!
//! Split from `task.rs` to keep that module under the 300-line cap and
//! to localize the lenient-serde logic in one place.

use crate::error::{BallError, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

/// Typed relationship kinds between tasks.
///
/// `Gates` is the post-review blocker: a parent holding `Gates` links
/// cannot transition to `Closed` while any referenced target is still
/// present in the store. See `Store::open_gate_blockers`.
///
/// `Unknown(String)` exists purely for forward compatibility: if a
/// newer version of `bl` writes a variant we don't recognize, older
/// clients round-trip it verbatim instead of hard-erroring on the whole
/// task file. `LinkType::parse` (the CLI entry point) never produces
/// `Unknown` — users cannot craft one by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkType {
    RelatesTo,
    Duplicates,
    Supersedes,
    RepliesTo,
    Gates,
    Unknown(String),
}

impl LinkType {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "relates_to" => Ok(LinkType::RelatesTo),
            "duplicates" => Ok(LinkType::Duplicates),
            "supersedes" => Ok(LinkType::Supersedes),
            "replies_to" => Ok(LinkType::RepliesTo),
            "gates" => Ok(LinkType::Gates),
            _ => Err(BallError::InvalidTask(format!("unknown link type: {s}"))),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            LinkType::RelatesTo => "relates_to",
            LinkType::Duplicates => "duplicates",
            LinkType::Supersedes => "supersedes",
            LinkType::RepliesTo => "replies_to",
            LinkType::Gates => "gates",
            LinkType::Unknown(s) => s.as_str(),
        }
    }
}

impl fmt::Display for LinkType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for LinkType {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LinkType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "relates_to" => LinkType::RelatesTo,
            "duplicates" => LinkType::Duplicates,
            "supersedes" => LinkType::Supersedes,
            "replies_to" => LinkType::RepliesTo,
            "gates" => LinkType::Gates,
            _ => LinkType::Unknown(s),
        })
    }
}

/// Typed relationship between two tasks.
///
/// Equality and `Vec::contains` deliberately consider only `link_type`
/// and `target`: two link records pointing at the same target with the
/// same type are "the same relationship" regardless of metadata a
/// future `bl` may attach via `extra`. This keeps `link_add` /
/// `link_rm` semantics stable when an older client edits a task whose
/// links carry forward-compat fields it doesn't recognize.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub link_type: LinkType,
    pub target: String,
    /// Forward-compat passthrough. See `Task::extra` for the rationale.
    /// Kept out of `PartialEq` so unknown fields never split a link
    /// into "same target, different metadata" duplicates.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.link_type == other.link_type && self.target == other.target
    }
}

impl Eq for Link {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_variants() {
        assert_eq!(LinkType::parse("relates_to").unwrap(), LinkType::RelatesTo);
        assert_eq!(LinkType::parse("duplicates").unwrap(), LinkType::Duplicates);
        assert_eq!(LinkType::parse("supersedes").unwrap(), LinkType::Supersedes);
        assert_eq!(LinkType::parse("replies_to").unwrap(), LinkType::RepliesTo);
        assert_eq!(LinkType::parse("gates").unwrap(), LinkType::Gates);
    }

    #[test]
    fn parse_rejects_unknown_cli_input() {
        // parse() is the CLI entry point — it must refuse unknown
        // strings so `bl link add` can't silently create Unknown
        // variants. Only deserialization produces Unknown.
        assert!(LinkType::parse("bogus").is_err());
        assert!(LinkType::parse("").is_err());
    }

    #[test]
    fn as_str_known_variants() {
        assert_eq!(LinkType::RelatesTo.as_str(), "relates_to");
        assert_eq!(LinkType::Duplicates.as_str(), "duplicates");
        assert_eq!(LinkType::Supersedes.as_str(), "supersedes");
        assert_eq!(LinkType::RepliesTo.as_str(), "replies_to");
        assert_eq!(LinkType::Gates.as_str(), "gates");
    }

    #[test]
    fn as_str_unknown_echoes_original() {
        let lt = LinkType::Unknown("future_variant".to_string());
        assert_eq!(lt.as_str(), "future_variant");
    }

    #[test]
    fn display_impl() {
        assert_eq!(format!("{}", LinkType::RelatesTo), "relates_to");
        assert_eq!(format!("{}", LinkType::Gates), "gates");
        assert_eq!(
            format!("{}", LinkType::Unknown("xyz".into())),
            "xyz"
        );
    }

    #[test]
    fn serialize_round_trip_all_known() {
        for lt in [
            LinkType::RelatesTo,
            LinkType::Duplicates,
            LinkType::Supersedes,
            LinkType::RepliesTo,
            LinkType::Gates,
        ] {
            let s = serde_json::to_string(&lt).unwrap();
            let back: LinkType = serde_json::from_str(&s).unwrap();
            assert_eq!(back, lt);
        }
    }

    #[test]
    fn deserialize_unknown_preserves_string() {
        // Forward-compat: an older binary reading a JSON file written
        // by a future bl version must not hard-error. The unknown
        // variant is preserved verbatim and re-serializes unchanged.
        let back: LinkType = serde_json::from_str("\"from_the_future\"").unwrap();
        assert_eq!(back, LinkType::Unknown("from_the_future".to_string()));
        let s = serde_json::to_string(&back).unwrap();
        assert_eq!(s, "\"from_the_future\"");
    }

    fn mk(lt: LinkType, target: &str) -> Link {
        Link { link_type: lt, target: target.into(), extra: BTreeMap::new() }
    }

    #[test]
    fn link_equality() {
        let a = mk(LinkType::RelatesTo, "bl-x");
        let b = mk(LinkType::RelatesTo, "bl-x");
        let c = mk(LinkType::Duplicates, "bl-x");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn link_equality_ignores_extra() {
        // A future bl may attach metadata to a link via the
        // forward-compat `extra` map. `link_add` / `link_rm` and the
        // `Vec::contains` check that drives them must continue to treat
        // two links with the same type+target as the same link, or an
        // older client editing the task would silently duplicate them.
        let mut a = mk(LinkType::Gates, "bl-y");
        let b = mk(LinkType::Gates, "bl-y");
        a.extra.insert("added_by".into(), serde_json::json!("alice"));
        assert_eq!(a, b);
    }

    #[test]
    fn link_unknown_field_round_trips() {
        // Forward-compat: a Link JSON written by a future bl with an
        // unknown sibling field must deserialize into `extra` and
        // re-serialize unchanged.
        let raw = r#"{"link_type":"gates","target":"bl-z","added_by":"alice"}"#;
        let l: Link = serde_json::from_str(raw).unwrap();
        assert_eq!(l.link_type, LinkType::Gates);
        assert_eq!(l.target, "bl-z");
        assert_eq!(
            l.extra.get("added_by").and_then(|v| v.as_str()),
            Some("alice"),
        );
        let s = serde_json::to_string(&l).unwrap();
        assert!(s.contains("\"added_by\":\"alice\""));
    }

    #[test]
    fn link_empty_extra_is_omitted_on_save() {
        // Round-trip safety: a link with no extras must serialize to
        // exactly the same shape as before this field existed, so older
        // tasks on disk see no diff churn just from a load+save cycle.
        let l = mk(LinkType::RelatesTo, "bl-w");
        let s = serde_json::to_string(&l).unwrap();
        assert!(!s.contains("extra"));
        assert_eq!(s, r#"{"link_type":"relates_to","target":"bl-w"}"#);
    }

    #[test]
    fn gates_link_serializes_as_plain_string() {
        let link = mk(LinkType::Gates, "bl-child");
        let s = serde_json::to_string(&link).unwrap();
        assert!(s.contains("\"gates\""));
        assert!(s.contains("\"bl-child\""));
    }
}
