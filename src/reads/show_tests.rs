//! Tests for `bl show <id>` — the full-ball field block and `--json` detail.

use super::*;
use crate::reads::test_support::{blocker, catalog, task};
use crate::reads::{Flags, Style};
use crate::task::{On, Task};

fn flags(json: bool, target: &str) -> Flags {
    Flags { json, plain: true, status: None, target: Some(target.into()) }
}

fn plain() -> Style {
    Style { plain: true }
}

/// A fully-populated ball: every optional field, a blocker, and a body.
fn rich_task() -> Task {
    let mut t = task("Refactor", 0);
    t.claimant = Some("alice".into());
    t.priority = Some(2);
    t.parent = Some("bl-root".into());
    t.tags = vec!["infra".into(), "refactor".into()];
    t.blockers = vec![blocker("bl-dep", On::Claim), blocker("bl-gate", On::Close)];
    t.body = "Some body text.".into();
    t
}

#[test]
fn show_renders_every_present_field_and_the_body() {
    let cat = catalog(&[("bl-1", rich_task()), ("bl-kid", child_of("bl-1"))]);
    let out = render(&cat, &flags(false, "bl-1"), &plain()).unwrap();
    for fragment in [
        "bl-1  Refactor",
        "claimant alice",
        "priority 2",
        "parent   bl-root",
        "tags     infra, refactor",
        "  blockers\n    bl-dep (on claim)\n    bl-gate (on close)\n",
        "  children\n    ready    bl-kid",
        "Some body text.",
    ] {
        assert!(out.contains(fragment), "missing {fragment:?} in:\n{out}");
    }
}

/// A child pointing at `parent`.
fn child_of(parent: &str) -> Task {
    Task { parent: Some(parent.into()), ..task("Kid", 1) }
}

#[test]
fn show_omits_absent_optional_fields_blockers_children_and_body() {
    let cat = catalog(&[("bl-bare", task("Bare", 0))]);
    let out = render(&cat, &flags(false, "bl-bare"), &plain()).unwrap();
    assert!(out.contains("status   ready"));
    for absent in ["claimant", "priority", "parent", "tags", "blockers", "children"] {
        assert!(!out.contains(absent), "unexpected {absent:?} in:\n{out}");
    }
    // No body ⇒ no trailing blank line + text.
    assert!(out.ends_with("updated  1970-01-01T00:00:00Z\n"));
}

#[test]
fn show_json_is_the_bedrock_record_no_derived_children_body_or_status() {
    let cat = catalog(&[("bl-1", rich_task()), ("bl-kid", child_of("bl-1"))]);
    let out = render(&cat, &flags(true, "bl-1"), &plain()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    // Stored frontmatter round-trips; the i64 timestamp is literal.
    assert_eq!(v["id"], "bl-1");
    assert_eq!(v["parent"], "bl-root");
    assert!(v["created"].is_i64());
    // Derived/non-frontmatter fields are absent — the human render owns them.
    for derived in ["children", "body", "status"] {
        assert!(v.get(derived).is_none(), "bedrock must omit {derived}");
    }
}

#[test]
fn show_errors_when_the_id_is_unknown() {
    let cat = catalog(&[("bl-1", task("One", 0))]);
    assert!(render(&cat, &flags(false, "bl-404"), &plain()).is_err());
}
