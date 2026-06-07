//! Tests for `bl list` rendering, the §10 ordering, and the `--status` filter.

use super::*;
use crate::reads::test_support::{catalog, task};
use crate::reads::{Flags, Style};
use crate::task::{Status, Task};

/// Plain (glyph-free) flags, optionally JSON; no status filter.
fn flags(json: bool) -> Flags {
    Flags { json, plain: true, status: None, target: None }
}

/// Plain flags narrowed to one §3 status rung.
fn flags_status(status: Status) -> Flags {
    Flags { json: false, plain: true, status: Some(status), target: None }
}

fn plain() -> Style {
    Style { plain: true }
}

/// A ball with an explicit priority.
fn prioritised(title: &str, created: i64, p: i64) -> Task {
    Task { priority: Some(p), ..task(title, created) }
}

#[test]
fn list_renders_one_plain_line_per_ball_with_hints() {
    let mut claimed = task("Held", 1);
    claimed.claimant = Some("alice".into());
    let cat = catalog(&[("bl-1", prioritised("First", 1, 2)), ("bl-2", claimed)]);
    let out = render_list(&cat, &flags(false), &plain());
    assert_eq!(
        out,
        "ready    bl-1  First  p2\nclaimed  bl-2  Held  @alice\n"
    );
}

#[test]
fn list_json_is_an_array_of_objects() {
    let cat = catalog(&[("bl-1", task("One", 0))]);
    let out = render_list(&cat, &flags(true), &plain());
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[0]["id"], "bl-1");
    assert!(v.is_array());
}

#[test]
fn list_orders_every_invocation_by_priority_then_created_then_id() {
    // bl-d has no priority (sorts LAST); bl-a/bl-b share priority 1, broken by
    // created; bl-c is priority 2. Ordering is uniform — no filter needed.
    let cat = catalog(&[
        ("bl-d", task("NoPrio", 5)),
        ("bl-c", prioritised("P2", 1, 2)),
        ("bl-b", prioritised("P1-late", 9, 1)),
        ("bl-a", prioritised("P1-early", 1, 1)),
    ]);
    let out = render_list(&cat, &flags(false), &plain());
    let order: Vec<&str> = out.lines().map(|l| l.split_whitespace().nth(1).unwrap()).collect();
    assert_eq!(order, ["bl-a", "bl-b", "bl-c", "bl-d"]);
}

#[test]
fn status_ready_filter_omits_blocked_and_claimed_balls() {
    let mut held = task("Held", 1);
    held.claimant = Some("me".into());
    let cat = catalog(&[("bl-ready", task("R", 1)), ("bl-held", held)]);
    let out = render_list(&cat, &flags_status(Status::Ready), &plain());
    assert_eq!(out, "ready    bl-ready  R\n");
}

#[test]
fn status_claimed_filter_keeps_only_claimed_balls() {
    let mut held = task("Held", 1);
    held.claimant = Some("me".into());
    let cat = catalog(&[("bl-ready", task("R", 1)), ("bl-held", held)]);
    let out = render_list(&cat, &flags_status(Status::Claimed), &plain());
    assert_eq!(out, "claimed  bl-held  Held  @me\n");
}

#[test]
fn status_ready_json_emits_the_ordered_array() {
    let cat = catalog(&[("bl-2", prioritised("Second", 1, 2)), ("bl-1", prioritised("First", 1, 1))]);
    let f = Flags { json: true, plain: true, status: Some(Status::Ready), target: None };
    let out = render_list(&cat, &f, &plain());
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[0]["id"], "bl-1");
    assert_eq!(v[1]["id"], "bl-2");
}
