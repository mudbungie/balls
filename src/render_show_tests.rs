//! Coverage-targeted tests for the `bl show` renderer: header, meta
//! row, tags, description, and notes. The relations block has its own
//! file (`render_show_relations_tests.rs`); shared task/ctx builders
//! live in `render_show_test_support`.

use super::render;
use crate::render_show_test_support::{ctx_for, empty_delivery, mk, now_fixed};
use crate::task::Note;
use chrono::Duration;
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn header_carries_id_title_and_status() {
    let t = mk("bl-1", "Do thing");
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    let first = out.lines().next().unwrap();
    assert!(first.contains("bl-1"));
    assert!(first.contains("Do thing"));
    assert!(first.contains("[ ] open"));
}

#[test]
fn header_shows_claimed_badge_when_self() {
    let mut t = mk("bl-1", "mine");
    t.claimed_by = Some("me".into());
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("claimed by me"));
}

#[test]
fn header_no_claimed_badge_when_unclaimed() {
    let t = mk("bl-1", "free");
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(!out.contains("claimed by"));
}

#[test]
fn meta_row_includes_relative_timestamps() {
    let t = mk("bl-1", "t");
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("created: 2h ago"));
    assert!(out.contains("updated: 2h ago"));
}

#[test]
fn meta_row_type_label_renders_any_string() {
    // Including values `parse` rejects — the render path reads
    // whatever made it onto disk so forward-compat is covered.
    for label in ["epic", "task", "bug", "spike", "Spike With Space"] {
        let mut t = mk("bl-x", "t");
        t.task_type = serde_json::from_value(serde_json::json!(label)).unwrap();
        let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
        assert!(out.contains(&format!("type: {label}")), "missing {label}");
    }
}

#[test]
fn tags_line_omitted_when_no_tags() {
    let t = mk("bl-1", "t");
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(!out.contains("\n  tags:"));
}

#[test]
fn tags_line_renders_when_tagged() {
    let mut t = mk("bl-1", "t");
    t.tags = vec!["api".into(), "auth".into()];
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("tags: api, auth"));
}

#[test]
fn description_wrapped_to_columns() {
    let mut t = mk("bl-1", "t");
    t.description = "alpha beta gamma delta epsilon".into();
    let mut ctx = ctx_for();
    ctx.columns = 16;
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx);
    assert!(out.contains("  description"));
    let body_line_max = out
        .lines()
        .filter(|l| l.starts_with("    ") && !l.contains(':'))
        .map(str::len)
        .max()
        .unwrap_or(0);
    // Each wrapped body line ≤ ctx.columns.
    assert!(body_line_max <= ctx.columns);
}

#[test]
fn description_omitted_when_empty() {
    let t = mk("bl-1", "t");
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(!out.contains("  description"));
}

#[test]
fn notes_section_renders_oldest_first_with_count() {
    let mut t = mk("bl-1", "t");
    t.notes.push(Note {
        ts: now_fixed() - Duration::days(2),
        author: "alice".into(),
        text: "kicked off".into(),
        extra: BTreeMap::new(),
    });
    t.notes.push(Note {
        ts: now_fixed() - Duration::hours(2),
        author: "bob".into(),
        text: "please add tests".into(),
        extra: BTreeMap::new(),
    });
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("notes (2)"));
    let alice = out.find("alice").unwrap();
    let bob = out.find("bob").unwrap();
    assert!(alice < bob);
    assert!(out.contains("kicked off"));
}
