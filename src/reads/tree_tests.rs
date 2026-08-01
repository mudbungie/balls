//! Tests for the containment forest over the rendered set (bl-61e0): nesting,
//! the orphan-is-a-root invariant, per-level order, and the cycle guard.

use super::forest;

/// `forest` over `(id, parent)` pairs, rendered as one `depth:id` per entry —
/// the shape the assertions read.
fn walk(nodes: &[(&str, Option<&str>)]) -> Vec<String> {
    forest(nodes).into_iter().map(|(i, d)| format!("{d}:{}", nodes[i].0)).collect()
}

#[test]
fn a_child_nests_under_a_rendered_parent_at_the_parent_position() {
    // bl-b is listed BEFORE its parent by the §10 order; the forest pulls it
    // under bl-a, and bl-c (a grandchild) one level deeper again.
    let out = walk(&[("bl-b", Some("bl-a")), ("bl-a", None), ("bl-c", Some("bl-b"))]);
    assert_eq!(out, ["0:bl-a", "1:bl-b", "2:bl-c"]);
}

#[test]
fn a_child_whose_parent_is_not_rendered_is_itself_a_root() {
    // The one invariant: filtered-out, closed and foreign parents are the SAME
    // case — the parent is not in the set, so the row renders at depth 0, in
    // its §10 position, with no branch of its own.
    let out = walk(&[("bl-x", Some("bl-gone")), ("bl-y", None)]);
    assert_eq!(out, ["0:bl-x", "0:bl-y"]);
}

#[test]
fn siblings_keep_the_incoming_order_within_their_level() {
    // §10 order applies PER LEVEL: the walk is stable, so the caller's order
    // survives among siblings even as the levels interleave.
    let out = walk(&[
        ("bl-p", None),
        ("bl-1", Some("bl-p")),
        ("bl-q", None),
        ("bl-2", Some("bl-p")),
        ("bl-3", Some("bl-q")),
    ]);
    assert_eq!(out, ["0:bl-p", "1:bl-1", "1:bl-2", "0:bl-q", "1:bl-3"]);
}

#[test]
fn a_parent_cycle_still_renders_every_row_exactly_once() {
    // The store invariant is not trusted: a→b→a has NO root, so the second
    // sweep renders the cycle from its first member and the `seen` set stops
    // the walk. Totality is the property that matters.
    let out = walk(&[("bl-a", Some("bl-b")), ("bl-b", Some("bl-a")), ("bl-solo", None)]);
    assert_eq!(out, ["0:bl-solo", "0:bl-a", "1:bl-b"]);
}

#[test]
fn a_self_parenting_ball_renders_once_as_a_root() {
    let out = walk(&[("bl-self", Some("bl-self"))]);
    assert_eq!(out, ["0:bl-self"]);
}

#[test]
fn an_empty_set_renders_nothing() {
    assert!(walk(&[]).is_empty());
}
