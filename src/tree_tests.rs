//! Coverage-targeted tests for the parent-edge tree renderer:
//! forest/rooted construction, cycle detection, and the box-drawing
//! prefixes. `format_line` annotation coverage lives in
//! `tree_format_tests.rs`; shared builders in `tree_test_support`.

use super::*;
use crate::display::Display;
use crate::tree_test_support::mk;

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
