//! Coverage for `format_line`'s annotations: type label, blocked-by,
//! gates-parent, and cycle markers (unicode + ascii), plus the
//! `JsonNode` round-trip shape. Split from `tree_tests.rs`; shared
//! builders live in `tree_test_support`.

use super::*;
use crate::display::Display;
use crate::task::{Link, LinkType, Status, TaskType};
use crate::tree_test_support::{mk, mk_full};

#[test]
fn format_line_marks_epic_type() {
    let tasks = vec![mk_full("bl-e", None, &[], Status::Open, TaskType::epic())];
    let n = forest(&tasks).pop().unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(line.contains("[epic]"));
}

#[test]
fn format_line_no_type_label_for_task() {
    let tasks = vec![mk_full("bl-t", None, &[], Status::Open, TaskType::task())];
    let n = forest(&tasks).pop().unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("[epic]"));
}

#[test]
fn format_line_blocked_annotation_unicode() {
    let tasks = vec![
        mk_full("a", None, &[], Status::Open, TaskType::task()),
        mk_full("b", None, &["a"], Status::Open, TaskType::task()),
    ];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::styled());
    assert!(line.contains("⌀ blocked by a"));
}

#[test]
fn format_line_blocked_annotation_ascii_when_plain() {
    let tasks = vec![
        mk_full("a", None, &[], Status::Open, TaskType::task()),
        mk_full("b", None, &["a"], Status::Open, TaskType::task()),
    ];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(line.contains("[!] blocked by a"));
}

#[test]
fn format_line_no_blocker_when_dep_is_closed() {
    let tasks = vec![
        mk_full("a", None, &[], Status::Closed, TaskType::task()),
        mk_full("b", None, &["a"], Status::Open, TaskType::task()),
    ];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("blocked by"));
}

#[test]
fn format_line_no_blocker_for_missing_dep() {
    // Missing dep => archived/closed. No blocker annotation.
    let tasks = vec![mk_full("b", None, &["ghost"], Status::Open, TaskType::task())];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("blocked by"));
}

#[test]
fn format_line_gates_parent_unicode() {
    let mut parent = mk_full("p", None, &[], Status::Open, TaskType::task());
    parent.links.push(Link {
        link_type: LinkType::Gates,
        target: "c".into(),
        extra: std::collections::BTreeMap::new(),
    });
    let child = mk("c", Some("p"));
    let tasks = vec![parent, child];
    let n = rooted(&tasks, "c").unwrap();
    let line = format_line(&n, &tasks, Display::styled());
    assert!(line.contains("⛓ gates parent"));
}

#[test]
fn format_line_gates_parent_ascii_when_plain() {
    let mut parent = mk_full("p", None, &[], Status::Open, TaskType::task());
    parent.links.push(Link {
        link_type: LinkType::Gates,
        target: "c".into(),
        extra: std::collections::BTreeMap::new(),
    });
    let child = mk("c", Some("p"));
    let tasks = vec![parent, child];
    let n = rooted(&tasks, "c").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(line.contains("[G] gates parent"));
}

#[test]
fn format_line_no_gates_annotation_without_parent() {
    let tasks = vec![mk("solo", None)];
    let n = rooted(&tasks, "solo").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("gates parent"));
}

#[test]
fn format_line_no_gates_annotation_when_parent_missing() {
    // A child whose parent isn't in the task set: no gates annotation.
    let tasks = vec![mk("c", Some("ghost"))];
    let n = rooted(&tasks, "c").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("gates parent"));
}

#[test]
fn format_line_no_gates_annotation_when_parent_link_targets_other() {
    let mut parent = mk_full("p", None, &[], Status::Open, TaskType::task());
    parent.links.push(Link {
        link_type: LinkType::Gates,
        target: "other".into(),
        extra: std::collections::BTreeMap::new(),
    });
    let child = mk("c", Some("p"));
    let tasks = vec![parent, child];
    let n = rooted(&tasks, "c").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("gates parent"));
}

#[test]
fn format_line_cycle_annotation_unicode_and_ascii() {
    let mut a = mk("a", Some("b"));
    let mut b = mk("b", Some("a"));
    a.parent = Some("b".into());
    b.parent = Some("a".into());
    let tasks = vec![a, b];
    let root = rooted(&tasks, "a").unwrap();
    // Drill down to the cycle node.
    let b_node = &root.children[0];
    let cycle_node = &b_node.children[0];
    assert!(cycle_node.cycle);
    let u = format_line(cycle_node, &tasks, Display::styled());
    assert!(u.contains("↺ cycle"));
    let a_line = format_line(cycle_node, &tasks, Display::plain());
    assert!(a_line.contains("<- cycle"));
}

#[test]
fn render_tree_stops_descent_on_cycle() {
    let mut a = mk("a", Some("b"));
    let mut b = mk("b", Some("a"));
    a.parent = Some("b".into());
    b.parent = Some("a".into());
    let tasks = vec![a, b];
    let root = rooted(&tasks, "a").unwrap();
    let out = render_forest(&[root], &tasks, Display::plain());
    // Each id appears at most twice (own line + once as cycle marker).
    let count = out.matches("a  a").count();
    assert!(count <= 2);
}

#[test]
fn json_node_round_trip_shape() {
    let tasks = vec![mk("p", None), mk("c", Some("p"))];
    let n = rooted(&tasks, "p").unwrap();
    let j = JsonNode::from_node(&n);
    let s = serde_json::to_string(&j).unwrap();
    assert!(s.contains("\"id\":\"p\""));
    assert!(s.contains("\"children\""));
    assert!(s.contains("\"id\":\"c\""));
}
