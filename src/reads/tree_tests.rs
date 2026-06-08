//! Tests for `bl dep-tree` — the containment forest and blocker annotations.

use super::*;
use crate::reads::test_support::{blocker, catalog, task};
use crate::reads::{Flags, Style};
use crate::task::{On, Task};

fn flags(json: bool) -> Flags {
    Flags { json, plain: true, ..Default::default() }
}

fn plain() -> Style {
    Style { plain: true }
}

/// A child of `parent`.
fn child(title: &str, parent: &str) -> Task {
    Task { parent: Some(parent.into()), ..task(title, 1) }
}

#[test]
fn the_forest_nests_children_and_treats_a_dangling_parent_as_a_root() {
    let mut root = task("Root", 1);
    // Both blocker ids are absent ⇒ resolved, so the annotation lists the edges
    // independently of the (ready) status; a claim and a close edge.
    root.blockers = vec![blocker("bl-dep", On::Claim), blocker("bl-gate", On::Close)];
    let cat = catalog(&[
        ("bl-root", root),
        ("bl-child", child("Child", "bl-root")),
        ("bl-grand", child("Grand", "bl-child")),
        ("bl-orphan", child("Orphan", "bl-missing")), // parent absent ⇒ a root
    ]);
    let out = render(&cat, &flags(false), &plain());
    // Roots print in id order: bl-orphan before bl-root, each nesting its kids.
    assert_eq!(
        out,
        "ready    bl-orphan  Orphan\n\
         ready    bl-root  Root [needs bl-dep, gate bl-gate]\n  \
         ready    bl-child  Child\n    ready    bl-grand  Grand\n"
    );
}

#[test]
fn a_child_whose_parent_is_live_is_not_a_root() {
    let cat = catalog(&[("bl-p", task("Parent", 1)), ("bl-c", child("Child", "bl-p"))]);
    let out = render(&cat, &flags(false), &plain());
    // Child appears only nested under the parent, not as its own root line.
    assert_eq!(out.matches("bl-c ").count(), 1);
    assert!(out.starts_with("ready    bl-p  Parent\n  ready    bl-c"));
}

#[test]
fn a_blocker_on_a_third_op_is_annotated_by_its_op_token() {
    // `on` is ANY op (§10/§15): claim→needs, close→gate, anything else→its token.
    let mut root = task("Root", 1);
    root.blockers = vec![blocker("bl-u", On::Update)];
    let cat = catalog(&[("bl-root", root)]);
    let out = render(&cat, &flags(false), &plain());
    assert_eq!(out, "ready    bl-root  Root [update bl-u]\n");
}

#[test]
fn an_unblocked_node_has_no_annotation() {
    let cat = catalog(&[("bl-x", task("X", 1))]);
    let out = render(&cat, &flags(false), &plain());
    assert_eq!(out, "ready    bl-x  X\n");
}

#[test]
fn the_json_is_a_flat_bedrock_array_not_a_nested_tree() {
    let cat = catalog(&[("bl-root", task("Root", 1)), ("bl-child", child("Child", "bl-root"))]);
    let out = render(&cat, &flags(true), &plain());
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    // Every ball is a flat top-level record; the tree is reconstructable from
    // each record's stored `parent`, never pre-nested (§9 bedrock).
    assert_eq!(v.as_array().unwrap().len(), 2);
    assert_eq!(v[0]["id"], "bl-child");
    assert_eq!(v[0]["parent"], "bl-root");
    assert_eq!(v[1]["id"], "bl-root");
    assert!(v[0].get("children").is_none());
}
