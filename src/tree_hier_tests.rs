//! Tests for the dotted hierarchy annotation (`Node::hier_path`).
//!
//! The annotation is purely a display aid computed during tree build.
//! These tests pin its shape, its absence on roots, and its propagation
//! through `format_line` and `JsonNode`.

use super::*;
use crate::display::Display;
use crate::task::{NewTaskOpts, Status, Task};

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

#[test]
fn root_nodes_carry_empty_hier_path() {
    let tasks = vec![mk("bl-a", None), mk("bl-b", None)];
    let f = forest(&tasks);
    assert!(f.iter().all(|n| n.hier_path.is_empty()));
}

#[test]
fn children_get_dotted_sibling_index_in_id_sorted_order() {
    // Siblings sort by id: c1 < c2 < c3, so their paths are .1 .2 .3.
    let tasks = vec![
        mk("p", None),
        mk("c2", Some("p")),
        mk("c1", Some("p")),
        mk("c3", Some("p")),
    ];
    let root = rooted(&tasks, "p").unwrap();
    let paths: Vec<(&str, &str)> = root
        .children
        .iter()
        .map(|c| (c.task.id.as_str(), c.hier_path.as_str()))
        .collect();
    assert_eq!(paths, vec![("c1", ".1"), ("c2", ".2"), ("c3", ".3")]);
}

#[test]
fn grandchildren_extend_parent_path() {
    let tasks = vec![
        mk("p", None),
        mk("c1", Some("p")),
        mk("c2", Some("p")),
        mk("g", Some("c2")),
    ];
    let root = rooted(&tasks, "p").unwrap();
    let c2 = root.children.iter().find(|n| n.task.id == "c2").unwrap();
    assert_eq!(c2.hier_path, ".2");
    assert_eq!(c2.children[0].hier_path, ".2.1");
}

#[test]
fn rooted_subtree_numbers_from_its_own_root() {
    // Even if the root has an ancestor, `rooted(_, id)` renders as a
    // standalone tree — root path is empty, children start at .1.
    let tasks = vec![
        mk("grand", None),
        mk("p", Some("grand")),
        mk("c", Some("p")),
    ];
    let sub = rooted(&tasks, "p").unwrap();
    assert!(sub.hier_path.is_empty());
    assert_eq!(sub.children[0].hier_path, ".1");
}

#[test]
fn format_line_appends_path_for_children_but_not_roots() {
    let tasks = vec![mk("p", None), mk("c", Some("p"))];
    let root = rooted(&tasks, "p").unwrap();
    let root_line = format_line(&root, &tasks, Display::plain());
    let child_line = format_line(&root.children[0], &tasks, Display::plain());
    assert!(root_line.starts_with("p  p"), "root: {root_line}");
    assert!(child_line.starts_with("c .1  c"), "child: {child_line}");
}

#[test]
fn json_node_omits_empty_hier_path_and_emits_dotted_for_children() {
    let tasks = vec![mk("p", None), mk("c", Some("p"))];
    let root = rooted(&tasks, "p").unwrap();
    let j = JsonNode::from_node(&root);
    let s = serde_json::to_string(&j).unwrap();
    // Root has an empty path, so the field is omitted entirely.
    assert!(!s.contains("\"hier_path\":\"\""), "root JSON: {s}");
    // Child renders `.1`.
    assert!(s.contains("\"hier_path\":\".1\""), "child JSON: {s}");
}

#[test]
fn cycle_node_still_carries_its_assigned_path() {
    // A cycle short-circuits descent but the node itself was reached
    // via normal sibling indexing — the path should survive.
    let mut a = mk("a", Some("b"));
    let mut b = mk("b", Some("a"));
    a.parent = Some("b".into());
    b.parent = Some("a".into());
    let tasks = vec![a, b];
    let root = rooted(&tasks, "a").unwrap();
    let b_node = &root.children[0];
    let cycle_node = &b_node.children[0];
    assert!(cycle_node.cycle);
    assert_eq!(b_node.hier_path, ".1");
    assert_eq!(cycle_node.hier_path, ".1.1");
}
