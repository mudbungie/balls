//! Tests for `bl list` rendering, the §10 ordering, the `--status` filter, the
//! `--closed`/`--all` reach, and the compose-AND history filters.

use super::*;
use crate::reads::history::Dead;
use crate::reads::test_support::{catalog, task};
use crate::reads::{Flags, Reach, Retired, Style};
use crate::task::{Status, Task};

/// Plain (glyph-free) flags, optionally JSON; no status filter.
fn flags(json: bool) -> Flags {
    Flags { json, plain: true, ..Default::default() }
}

/// Plain flags narrowed to one §3 status rung.
fn flags_status(status: Status) -> Flags {
    Flags { plain: true, status: Some(status), ..Default::default() }
}

fn plain() -> Style {
    Style { plain: true }
}

/// A reconstructed dead ball, for the reach/render tests.
fn dead(id: &str, title: &str, created: i64, retired: Retired) -> Dead {
    Dead { id: id.into(), task: task(title, created), retired, retired_at: created + 1 }
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
    let out = render_list(&cat, &[], &flags(false), &plain());
    assert_eq!(
        out,
        "ready    bl-1  First  p2\nclaimed  bl-2  Held  @alice\n"
    );
}

#[test]
fn list_json_is_an_array_of_objects() {
    let cat = catalog(&[("bl-1", task("One", 0))]);
    let out = render_list(&cat, &[], &flags(true), &plain());
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
    let out = render_list(&cat, &[], &flags(false), &plain());
    let order: Vec<&str> = out.lines().map(|l| l.split_whitespace().nth(1).unwrap()).collect();
    assert_eq!(order, ["bl-a", "bl-b", "bl-c", "bl-d"]);
}

#[test]
fn status_ready_filter_omits_blocked_and_claimed_balls() {
    let mut held = task("Held", 1);
    held.claimant = Some("me".into());
    let cat = catalog(&[("bl-ready", task("R", 1)), ("bl-held", held)]);
    let out = render_list(&cat, &[], &flags_status(Status::Ready), &plain());
    assert_eq!(out, "ready    bl-ready  R\n");
}

#[test]
fn status_claimed_filter_keeps_only_claimed_balls() {
    let mut held = task("Held", 1);
    held.claimant = Some("me".into());
    let cat = catalog(&[("bl-ready", task("R", 1)), ("bl-held", held)]);
    let out = render_list(&cat, &[], &flags_status(Status::Claimed), &plain());
    assert_eq!(out, "claimed  bl-held  Held  @me\n");
}

#[test]
fn status_ready_json_emits_the_ordered_array() {
    let cat = catalog(&[("bl-2", prioritised("Second", 1, 2)), ("bl-1", prioritised("First", 1, 1))]);
    let f = Flags { json: true, plain: true, status: Some(Status::Ready), ..Default::default() };
    let out = render_list(&cat, &[], &f, &plain());
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[0]["id"], "bl-1");
    assert_eq!(v[1]["id"], "bl-2");
}

/// Plain flags at a given reach.
fn flags_reach(reach: Reach) -> Flags {
    Flags { plain: true, reach, ..Default::default() }
}

#[test]
fn the_default_reach_omits_the_dead_set() {
    let cat = catalog(&[("bl-live", task("Live", 1))]);
    let dead_set = [dead("bl-dead", "Dead", 2, Retired::Closed)];
    // reach=Live: the dead slice is present but never reached.
    let out = render_list(&cat, &dead_set, &flags(false), &plain());
    assert_eq!(out, "ready    bl-live  Live\n");
}

#[test]
fn closed_reach_shows_only_the_dead_set() {
    let cat = catalog(&[("bl-live", task("Live", 1))]);
    let dead_set = [dead("bl-c", "Closed", 2, Retired::Closed), dead("bl-x", "Dropped", 3, Retired::Dropped)];
    let out = render_list(&cat, &dead_set, &flags_reach(Reach::Dead), &plain());
    assert_eq!(out, "closed   bl-c  Closed\ndropped  bl-x  Dropped\n");
}

#[test]
fn all_reach_interleaves_live_and_dead_by_the_uniform_order() {
    // created drives the order across both sets (no priorities here).
    let cat = catalog(&[("bl-live", task("Live", 2))]);
    let dead_set = [dead("bl-old", "Old", 1, Retired::Closed), dead("bl-new", "New", 3, Retired::Dropped)];
    let out = render_list(&cat, &dead_set, &flags_reach(Reach::All), &plain());
    assert_eq!(out, "closed   bl-old  Old\nready    bl-live  Live\ndropped  bl-new  New\n");
}

#[test]
fn all_reach_json_emits_one_bedrock_array_over_both_sets() {
    let cat = catalog(&[("bl-live", task("Live", 2))]);
    let dead_set = [dead("bl-dead", "Dead", 1, Retired::Closed)];
    let f = Flags { json: true, plain: true, reach: Reach::All, ..Default::default() };
    let out = render_list(&cat, &dead_set, &f, &plain());
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[0]["id"], "bl-dead"); // created=1, sorts first
    assert_eq!(v[1]["id"], "bl-live");
}

#[test]
fn a_tag_filter_narrows_both_live_and_dead() {
    let mut tagged_live = task("Tagged live", 1);
    tagged_live.tags = vec!["keep".into()];
    let cat = catalog(&[("bl-keep", tagged_live), ("bl-drop", task("Untagged", 2))]);
    let mut d = dead("bl-dkeep", "Tagged dead", 3, Retired::Closed);
    d.task.tags = vec!["keep".into()];
    let dead_set = [d, dead("bl-dother", "Untagged dead", 4, Retired::Closed)];
    let f = Flags { plain: true, reach: Reach::All, tags: vec!["keep".into()], ..Default::default() };
    let out = render_list(&cat, &dead_set, &f, &plain());
    assert_eq!(out, "ready    bl-keep  Tagged live\nclosed   bl-dkeep  Tagged dead\n");
}

#[test]
fn a_text_filter_searches_the_live_set() {
    let cat = catalog(&[("bl-1", task("Refactor auth", 1)), ("bl-2", task("Add caching", 2))]);
    let f = Flags { plain: true, target: Some("auth".into()), ..Default::default() };
    let out = render_list(&cat, &[], &f, &plain());
    assert_eq!(out, "ready    bl-1  Refactor auth\n");
}

#[test]
fn a_dead_ball_date_window_reads_its_deletion_date() {
    // The dead ball was created at 1 but retired_at = created + 1 = 2; a window
    // that excludes both created and retired drops it.
    let dead_set = [dead("bl-d", "Dead", 1, Retired::Closed)];
    let in_win = Flags { plain: true, reach: Reach::Dead, since: Some(2), until: Some(2 + 86_399), ..Default::default() };
    assert!(render_list(&catalog(&[]), &dead_set, &in_win, &plain()).contains("bl-d"));
    let out_win = Flags { plain: true, reach: Reach::Dead, since: Some(100), ..Default::default() };
    assert!(render_list(&catalog(&[]), &dead_set, &out_win, &plain()).is_empty());
}
