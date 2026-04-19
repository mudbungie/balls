//! Coverage-targeted tests for the parent-edge tree renderer.

use super::*;
use crate::display::Display;
use crate::task::{Link, LinkType, NewTaskOpts, Status, Task, TaskType};

fn mk(id: &str, parent: Option<&str>) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: id.into(),
            parent: parent.map(String::from),
            ..Default::default()
        },
        id.into(),
    );
    t.status = Status::Open;
    t
}

fn mk_full(
    id: &str,
    parent: Option<&str>,
    deps: &[&str],
    status: Status,
    ttype: TaskType,
) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: id.into(),
            parent: parent.map(String::from),
            depends_on: deps.iter().map(|s| String::from(*s)).collect(),
            task_type: ttype,
            ..Default::default()
        },
        id.into(),
    );
    t.status = status;
    t
}

#[test]
fn forest_collects_parentless_roots_sorted() {
    let tasks = vec![mk("bl-2", None), mk("bl-1", None), mk("bl-3", Some("bl-1"))];
    let f = forest(&tasks);
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].task.id, "bl-1");
    assert_eq!(f[1].task.id, "bl-2");
    // bl-1 carries bl-3 as a child (parent edge).
    assert_eq!(f[0].children.len(), 1);
    assert_eq!(f[0].children[0].task.id, "bl-3");
}

#[test]
fn rooted_returns_some_for_known_id_and_none_for_missing() {
    let tasks = vec![mk("bl-1", None), mk("bl-2", Some("bl-1"))];
    let n = rooted(&tasks, "bl-1").unwrap();
    assert_eq!(n.children.len(), 1);
    assert!(rooted(&tasks, "bl-ghost").is_none());
}

#[test]
fn build_detects_cycle_in_parent_chain() {
    // Hand-craft a parent cycle: a's parent is b, b's parent is a.
    let mut a = mk("a", Some("b"));
    let mut b = mk("b", Some("a"));
    a.parent = Some("b".into());
    b.parent = Some("a".into());
    let tasks = vec![a, b];
    // No parentless roots — `forest` is empty. Use rooted to enter the cycle.
    let n = rooted(&tasks, "a").unwrap();
    // descend into b, which loops back to a — second visit marks cycle.
    let b_child = &n.children[0];
    assert_eq!(b_child.task.id, "b");
    assert!(b_child.children[0].cycle);
}

#[test]
fn render_forest_separates_roots_with_blank_line() {
    let tasks = vec![mk("bl-1", None), mk("bl-2", None)];
    let f = forest(&tasks);
    let out = render_forest(&f, &tasks, Display::plain());
    // Two root lines + blank line in between.
    let lines: Vec<&str> = out.split('\n').collect();
    assert!(lines[0].contains("bl-1"));
    assert_eq!(lines[1], "");
    assert!(lines[2].contains("bl-2"));
}

#[test]
fn render_forest_empty_is_empty_string() {
    let out = render_forest(&[], &[], Display::plain());
    assert!(out.is_empty());
}

#[test]
fn render_tree_uses_box_prefix_for_children_unicode() {
    let tasks = vec![mk("p", None), mk("c1", Some("p")), mk("c2", Some("p"))];
    let f = forest(&tasks);
    let out = render_forest(&f, &tasks, Display::styled());
    assert!(out.contains("├─ "));
    assert!(out.contains("└─ "));
}

#[test]
fn render_tree_uses_ascii_prefix_when_plain() {
    let tasks = vec![
        mk("p", None),
        mk("c1", Some("p")),
        mk("c2", Some("p")),
        mk("g1", Some("c1")),
    ];
    let f = forest(&tasks);
    let out = render_forest(&f, &tasks, Display::plain());
    // ASCII tee, corner, and either pad or vbar must appear.
    assert!(out.contains("|- "));
    assert!(out.contains("`- "));
    // grandchild under non-last child uses vbar continuation.
    assert!(out.contains("|  "));
}

#[test]
fn render_tree_pads_under_last_child_for_grandchildren() {
    // If c2 is the LAST child, its subtree's prefix should use spaces
    // (pad) rather than the vertical bar — tests the `anc_last` true
    // branch in `tree_prefix`.
    let tasks = vec![
        mk("p", None),
        mk("c1", Some("p")),
        mk("c2", Some("p")),
        mk("g_under_c2", Some("c2")),
    ];
    let f = forest(&tasks);
    let out = render_forest(&f, &tasks, Display::plain());
    // Grandchild line should have "   " (4-space pad) before its corner.
    assert!(out.lines().any(|l| l.starts_with("   `- ") && l.contains("g_under_c2")));
}

#[test]
fn format_line_marks_epic_type() {
    let tasks = vec![mk_full("bl-e", None, &[], Status::Open, TaskType::Epic)];
    let n = forest(&tasks).pop().unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(line.contains("[epic]"));
}

#[test]
fn format_line_no_type_label_for_task() {
    let tasks = vec![mk_full("bl-t", None, &[], Status::Open, TaskType::Task)];
    let n = forest(&tasks).pop().unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("[epic]"));
}

#[test]
fn format_line_blocked_annotation_unicode() {
    let tasks = vec![
        mk_full("a", None, &[], Status::Open, TaskType::Task),
        mk_full("b", None, &["a"], Status::Open, TaskType::Task),
    ];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::styled());
    assert!(line.contains("⌀ blocked by a"));
}

#[test]
fn format_line_blocked_annotation_ascii_when_plain() {
    let tasks = vec![
        mk_full("a", None, &[], Status::Open, TaskType::Task),
        mk_full("b", None, &["a"], Status::Open, TaskType::Task),
    ];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(line.contains("[!] blocked by a"));
}

#[test]
fn format_line_no_blocker_when_dep_is_closed() {
    let tasks = vec![
        mk_full("a", None, &[], Status::Closed, TaskType::Task),
        mk_full("b", None, &["a"], Status::Open, TaskType::Task),
    ];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("blocked by"));
}

#[test]
fn format_line_no_blocker_for_missing_dep() {
    // Missing dep => archived/closed. No blocker annotation.
    let tasks = vec![mk_full("b", None, &["ghost"], Status::Open, TaskType::Task)];
    let n = rooted(&tasks, "b").unwrap();
    let line = format_line(&n, &tasks, Display::plain());
    assert!(!line.contains("blocked by"));
}

#[test]
fn format_line_gates_parent_unicode() {
    let mut parent = mk_full("p", None, &[], Status::Open, TaskType::Task);
    parent.links.push(Link {
        link_type: LinkType::Gates,
        target: "c".into(),
    });
    let child = mk("c", Some("p"));
    let tasks = vec![parent, child];
    let n = rooted(&tasks, "c").unwrap();
    let line = format_line(&n, &tasks, Display::styled());
    assert!(line.contains("⛓ gates parent"));
}

#[test]
fn format_line_gates_parent_ascii_when_plain() {
    let mut parent = mk_full("p", None, &[], Status::Open, TaskType::Task);
    parent.links.push(Link {
        link_type: LinkType::Gates,
        target: "c".into(),
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
    let mut parent = mk_full("p", None, &[], Status::Open, TaskType::Task);
    parent.links.push(Link {
        link_type: LinkType::Gates,
        target: "other".into(),
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
