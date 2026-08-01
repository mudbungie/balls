//! The default human render as the containment TREE (bl-61e0) — a nested child
//! of [`super`] (the `list` render tests), inheriting its fixtures (`NOW`,
//! `nostore`, `nogit_xdg`, `flags`, `flags_status`, `flags_reach`, `plain`,
//! `render`, `dead`, `prioritised`, `catalog`, `task`, `Ctx`, `render_list`, …)
//! through `use super::*`.

use super::*;

/// A ready ball contained under `parent` (containment only — no close-gate, so
/// no `->` delivery marker rides along).
fn child(title: &str, created: i64, parent: &str) -> Task {
    Task { parent: Some(parent.into()), ..task(title, created) }
}

#[test]
fn a_child_renders_indented_under_its_parent() {
    // The default view: bl-kid would sort FIRST flat (p1 beats the parent's p3),
    // and instead renders one level under bl-epic — containment beats the global
    // priority scan, the accepted trade (a child is meaningless out of context).
    let mut kid = child("Kid", 2, "bl-epic");
    kid.priority = Some(1);
    let cat = catalog(&[("bl-epic", prioritised("Epic", 1, 3)), ("bl-kid", kid)]);
    let out = render(&cat, &[], &flags(false), &plain());
    assert_eq!(out, "ready    bl-epic  Epic  p3\n  ready    bl-kid  Kid  p1\n");
}

#[test]
fn a_grandchild_indents_one_level_per_rendered_ancestor() {
    let cat = catalog(&[
        ("bl-a", task("Top", 1)),
        ("bl-b", child("Mid", 2, "bl-a")),
        ("bl-c", child("Leaf", 3, "bl-b")),
    ]);
    let out = render(&cat, &[], &flags(false), &plain());
    assert_eq!(
        out,
        "ready    bl-a  Top\n  ready    bl-b  Mid\n    ready    bl-c  Leaf\n"
    );
}

#[test]
fn a_child_whose_parent_the_filter_dropped_renders_as_a_root() {
    // The forest is over the RENDERED set: `-s ready` drops the claimed parent,
    // so its child renders flush left in its own §10 position. A closed parent
    // (no file) and a foreign-scoped one take the identical path — no branch.
    let mut held = task("Held epic", 1);
    held.claimant = Some("alice".into());
    let cat =
        catalog(&[("bl-epic", held), ("bl-kid", child("Kid", 2, "bl-epic")), ("bl-gone", child("Orphan", 3, "bl-dead"))]);
    let out = render(&cat, &[], &flags_status(Status::Ready), &plain());
    assert_eq!(out, "ready    bl-kid  Kid\nready    bl-gone  Orphan\n");
}

#[test]
fn the_order_applies_per_sibling_level() {
    // §10 order (priority, then created, then id) inside each level: bl-p's kids
    // order p1 before p2, and the roots keep their own order around the subtree.
    let mut kid_late = child("Late kid", 2, "bl-p");
    kid_late.priority = Some(2);
    let mut kid_early = child("Early kid", 3, "bl-p");
    kid_early.priority = Some(1);
    let cat = catalog(&[
        ("bl-p", prioritised("Parent", 1, 1)),
        ("bl-late", kid_late),
        ("bl-early", kid_early),
        ("bl-z", prioritised("Other root", 4, 2)),
    ]);
    let out = render(&cat, &[], &flags(false), &plain());
    let order: Vec<&str> = out.lines().map(|l| l.split_whitespace().nth(1).unwrap()).collect();
    assert_eq!(order, ["bl-p", "bl-early", "bl-late", "bl-z"]);
}

#[test]
fn a_dead_child_nests_under_a_live_parent_in_the_all_reach() {
    // Live and dead rows are one set to the forest — the reach chooses the set,
    // the tree shapes whatever it gets.
    let cat = catalog(&[("bl-epic", task("Epic", 1))]);
    let gone = child("Done kid", 2, "bl-epic");
    let dead_set = [Dead { id: "bl-done".into(), task: gone, retired_at: 3 }];
    let out = render(&cat, &dead_set, &flags_reach(Reach::All), &plain());
    assert_eq!(out, "ready    bl-epic  Epic\n  closed   bl-done  Done kid\n");
}

#[test]
fn a_parent_cycle_renders_every_row_once() {
    // The store invariant is not trusted (§10 has no cycle guard of its own):
    // a↔b renders totally, each row exactly once, no unbounded walk.
    let cat = catalog(&[("bl-a", child("A", 1, "bl-b")), ("bl-b", child("B", 2, "bl-a"))]);
    let out = render(&cat, &[], &flags(false), &plain());
    assert_eq!(out, "ready    bl-a  A\n  ready    bl-b  B\n");
}

#[test]
fn json_stays_flat_and_in_the_global_order() {
    // The bedrock mirror returns BEFORE the tree: no indentation, no reshuffle,
    // no `parent`-driven grouping — the child still sorts first by p1 (§3).
    let mut kid = child("Kid", 2, "bl-epic");
    kid.priority = Some(1);
    let cat = catalog(&[("bl-epic", prioritised("Epic", 1, 3)), ("bl-kid", kid)]);
    let out = render(&cat, &[], &flags(true), &plain());
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[0]["id"], "bl-kid");
    assert_eq!(v[1]["id"], "bl-epic");
    assert_eq!(v.as_array().unwrap().len(), 2);
}
