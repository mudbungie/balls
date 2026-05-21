//! Coverage-targeted tests for the `bl show` relations block (deps,
//! gates, links, parent, children, delivered, branch, repo, external,
//! dep_blocked). Driven through the public `render` entry point.

use crate::delivery::Delivery;
use crate::link::{Link, LinkType};
use crate::render_show::render;
use crate::render_show_test_support::{ctx_for, empty_delivery, mk, now_fixed};
use crate::task::ArchivedChild;
use std::collections::BTreeMap;
use std::path::Path;

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
        extra: std::collections::BTreeMap::new(),
    });
    t.links.push(Link {
        link_type: LinkType::Gates,
        target: "bl-x".into(),
        extra: std::collections::BTreeMap::new(),
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
        extra: std::collections::BTreeMap::new(),
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
        ..Delivery::default()
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
fn relations_block_omitted_when_no_relations() {
    let t = mk("bl-1", "lonely");
    let out = render(&t, std::slice::from_ref(&t), &empty_delivery(), Path::new("."), &ctx_for());
    // No "deps:", "gates:", "parent:", "children:" lines.
    for keyword in ["deps:", "gates:", "parent:", "children:", "delivered:", "branch:", "remote:"] {
        assert!(!out.contains(keyword), "unexpected {keyword}");
    }
}
