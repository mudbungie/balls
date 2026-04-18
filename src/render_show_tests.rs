//! Coverage-targeted tests for the `bl show` renderer.

use super::*;
use crate::delivery::Delivery;
use crate::display::Display;
use crate::link::{Link, LinkType};
use crate::task::{ArchivedChild, NewTaskOpts, Note, Status, Task, TaskType};
use chrono::{Duration, TimeZone, Utc};
use std::collections::BTreeMap;
use std::path::Path;

fn now_fixed() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap()
}

fn ctx_for<'a>() -> Ctx<'a> {
    Ctx {
        d: Display::plain(),
        me: "me",
        columns: 80,
        verbose: false,
        now: now_fixed(),
    }
}

fn empty_delivery() -> Delivery {
    Delivery { sha: None, hint_stale: false }
}

fn mk(id: &str, title: &str) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: title.into(),
            ..Default::default()
        },
        id.into(),
    );
    t.status = Status::Open;
    let when = now_fixed() - Duration::hours(2);
    t.created_at = when;
    t.updated_at = when;
    t
}

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
fn meta_row_type_label_for_each_variant() {
    for (variant, label) in [
        (TaskType::Epic, "epic"),
        (TaskType::Task, "task"),
        (TaskType::Bug, "bug"),
        (TaskType::Unknown("spike".into()), "spike"),
    ] {
        let mut t = mk("bl-x", "t");
        t.task_type = variant;
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
fn deps_render_with_inline_status_for_known_and_archived() {
    let known = mk("bl-d", "dep");
    let mut t = mk("bl-1", "t");
    t.depends_on = vec!["bl-d".into(), "bl-ghost".into()];
    let all = vec![t.clone(), known];
    let out = render(&t, &all, &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("bl-d [ ] open"));
    assert!(out.contains("bl-ghost (archived)"));
}

#[test]
fn gates_section_renders_known_and_unknown() {
    let target = mk("bl-g", "gate");
    let mut t = mk("bl-1", "t");
    t.links.push(Link {
        link_type: LinkType::Gates,
        target: "bl-g".into(),
    });
    t.links.push(Link {
        link_type: LinkType::Gates,
        target: "bl-x".into(),
    });
    let all = vec![t.clone(), target];
    let out = render(&t, &all, &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("bl-g [ ] open"));
    assert!(out.contains("bl-x (closed)"));
}

#[test]
fn parent_line_includes_parent_title_when_known() {
    let parent = mk("bl-p", "Parent");
    let mut t = mk("bl-c", "child");
    t.parent = Some("bl-p".into());
    let all = vec![parent, t.clone()];
    let out = render(&t, &all, &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("parent:   bl-p  Parent"));
}

#[test]
fn parent_line_omits_title_when_parent_missing() {
    let mut t = mk("bl-c", "child");
    t.parent = Some("bl-ghost".into());
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("parent:   bl-ghost"));
    assert!(!out.contains("bl-ghost  "));
}

#[test]
fn children_section_lists_live_and_archived_with_completion() {
    let parent = mk("bl-p", "p");
    let mut child_open = mk("bl-c1", "alive");
    child_open.parent = Some("bl-p".into());
    let mut parent2 = parent.clone();
    parent2.closed_children.push(ArchivedChild {
        id: "bl-c2".into(),
        title: "ancient".into(),
        closed_at: now_fixed(),
    });
    let all = vec![parent2.clone(), child_open];
    let out = render(&parent2, &all, &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("bl-c1 [open] alive"));
    assert!(out.contains("bl-c2 [archived] ancient"));
    assert!(out.contains("completion: 50%"));
}

#[test]
fn delivered_line_renders_when_sha_present() {
    let t = mk("bl-1", "t");
    let d = Delivery {
        sha: Some("abcdef0".into()),
        hint_stale: true,
    };
    let out = render(&t, std::slice::from_ref(&t), &d, Path::new("."), &ctx_for());
    assert!(out.contains("delivered: abcdef0"));
    assert!(out.contains("(hint stale)"));
}

#[test]
fn branch_line_renders_when_branch_set() {
    let mut t = mk("bl-1", "t");
    t.branch = Some("work/bl-1".into());
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("branch:   work/bl-1"));
}

#[test]
fn external_remote_renders_with_key_and_url() {
    let mut t = mk("bl-1", "t");
    let mut blob = serde_json::Map::new();
    blob.insert("remote_key".into(), serde_json::json!("LIN-1"));
    blob.insert("remote_url".into(), serde_json::json!("https://x"));
    let mut ext = BTreeMap::new();
    ext.insert("linear".into(), serde_json::Value::Object(blob));
    t.external = ext;
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("remote:"));
    assert!(out.contains("linear: LIN-1 https://x"));
}

#[test]
fn external_skipped_when_blob_has_no_remote_fields() {
    let mut t = mk("bl-1", "t");
    let mut blob = serde_json::Map::new();
    blob.insert("internal".into(), serde_json::json!("ignored"));
    let mut ext = BTreeMap::new();
    ext.insert("plug".into(), serde_json::Value::Object(blob));
    t.external = ext;
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    assert!(!out.contains("remote:"));
}

#[test]
fn dep_blocked_line_renders_when_open_dep_present() {
    let blocker = mk("bl-b", "blocker");
    let mut t = mk("bl-1", "t");
    t.depends_on = vec!["bl-b".into()];
    let all = vec![blocker, t.clone()];
    let out = render(&t, &all, &empty_delivery(), Path::new("."), &ctx_for());
    assert!(out.contains("dep_blocked: yes"));
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

#[test]
fn relations_block_omitted_when_no_relations() {
    let t = mk("bl-1", "lonely");
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    // No "deps:", "gates:", "parent:", "children:" lines.
    for keyword in ["deps:", "gates:", "parent:", "children:", "delivered:", "branch:", "remote:"] {
        assert!(!out.contains(keyword), "unexpected {keyword}");
    }
}
