//! Tests for `bl list` rendering, the §10 ordering, the `--status` filter, the
//! `--status closed`/`--all` reach, and the compose-AND history filters.

use std::path::Path;

use super::*;
use crate::reads::history::Dead;
use crate::reads::test_support::{catalog, git_store, task};
use crate::reads::{Catalog, Flags, Reach, Style};
use crate::task::{Status, Task};

/// A fixed render clock — the derived claim-age is measured against it.
const NOW: i64 = 1_000_000;

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

/// A store path that is never walked — safe for rows that trigger no claim-age
/// walk (unclaimed live rows, dead rows, `--json`); a live claimed row would
/// error against it, which is the guard these tests rely on.
fn nostore() -> &'static Path {
    Path::new("/balls-no-such-store")
}

/// Render against a never-walked store — the shape for tests with no live
/// claimed row (which alone pays the walk).
fn render(cat: &Catalog, dead: &[Dead], flags: &Flags, style: &Style) -> String {
    render_list(cat, dead, flags, style, nostore(), NOW).unwrap()
}

/// A reconstructed dead ball, for the reach/render tests.
fn dead(id: &str, title: &str, created: i64) -> Dead {
    Dead { id: id.into(), task: task(title, created), retired_at: created + 1 }
}

/// A ball with an explicit priority.
fn prioritised(title: &str, created: i64, p: i64) -> Task {
    Task { priority: Some(p), ..task(title, created) }
}

#[test]
fn list_renders_one_plain_line_per_ball_with_hints() {
    // bl-2 is claimed; its `@alice` carries the derived claim-age (§9), read
    // from the store's newest `bl-op: claim` commit — here 2h before NOW.
    let s = git_store();
    s.create("bl-1", &prioritised("First", 1, 2), 1);
    let mut claimed = task("Held", 1);
    claimed.claimant = Some("alice".into());
    s.claim("bl-2", &claimed, NOW - 2 * 3_600);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = render_list(&cat, &[], &flags(false), &plain(), s.dir(), NOW).unwrap();
    assert_eq!(out, "ready    bl-1  First  p2\nclaimed  bl-2  Held  @alice (2h)\n");
}

#[test]
fn list_json_is_an_array_of_objects() {
    let cat = catalog(&[("bl-1", task("One", 0))]);
    let out = render(&cat, &[], &flags(true), &plain());
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
    let out = render(&cat, &[], &flags(false), &plain());
    let order: Vec<&str> = out.lines().map(|l| l.split_whitespace().nth(1).unwrap()).collect();
    assert_eq!(order, ["bl-a", "bl-b", "bl-c", "bl-d"]);
}

#[test]
fn status_ready_filter_omits_blocked_and_claimed_balls() {
    let mut held = task("Held", 1);
    held.claimant = Some("me".into());
    let cat = catalog(&[("bl-ready", task("R", 1)), ("bl-held", held)]);
    let out = render(&cat, &[], &flags_status(Status::Ready), &plain());
    assert_eq!(out, "ready    bl-ready  R\n");
}

#[test]
fn status_claimed_filter_keeps_only_claimed_balls() {
    let s = git_store();
    s.create("bl-ready", &task("R", 1), 1);
    let mut held = task("Held", 1);
    held.claimant = Some("me".into());
    s.claim("bl-held", &held, NOW - 3 * 3_600);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = render_list(&cat, &[], &flags_status(Status::Claimed), &plain(), s.dir(), NOW).unwrap();
    assert_eq!(out, "claimed  bl-held  Held  @me (3h)\n");
}

#[test]
fn status_ready_json_emits_the_ordered_array() {
    let cat = catalog(&[("bl-2", prioritised("Second", 1, 2)), ("bl-1", prioritised("First", 1, 1))]);
    let f = Flags { json: true, plain: true, status: Some(Status::Ready), ..Default::default() };
    let out = render(&cat, &[], &f, &plain());
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
    let dead_set = [dead("bl-dead", "Dead", 2)];
    // reach=Live: the dead slice is present but never reached.
    let out = render(&cat, &dead_set, &flags(false), &plain());
    assert_eq!(out, "ready    bl-live  Live\n");
}

#[test]
fn closed_reach_shows_only_the_dead_set() {
    // Both a closed and a dropped ball render `closed` — the verb that retired
    // them is not projected as a distinct status (the titles label the source).
    let cat = catalog(&[("bl-live", task("Live", 1))]);
    let dead_set = [dead("bl-c", "Was closed", 2), dead("bl-x", "Was dropped", 3)];
    let out = render(&cat, &dead_set, &flags_reach(Reach::Dead), &plain());
    assert_eq!(out, "closed   bl-c  Was closed\nclosed   bl-x  Was dropped\n");
}

#[test]
fn all_reach_interleaves_live_and_dead_by_the_uniform_order() {
    // created drives the order across both sets (no priorities here).
    let cat = catalog(&[("bl-live", task("Live", 2))]);
    let dead_set = [dead("bl-old", "Old", 1), dead("bl-new", "New", 3)];
    let out = render(&cat, &dead_set, &flags_reach(Reach::All), &plain());
    assert_eq!(out, "closed   bl-old  Old\nready    bl-live  Live\nclosed   bl-new  New\n");
}

#[test]
fn all_reach_json_emits_one_bedrock_array_over_both_sets() {
    let cat = catalog(&[("bl-live", task("Live", 2))]);
    let dead_set = [dead("bl-dead", "Dead", 1)];
    let f = Flags { json: true, plain: true, reach: Reach::All, ..Default::default() };
    let out = render(&cat, &dead_set, &f, &plain());
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[0]["id"], "bl-dead"); // created=1, sorts first
    assert_eq!(v[1]["id"], "bl-live");
}

#[test]
fn a_tag_filter_narrows_both_live_and_dead() {
    let mut tagged_live = task("Tagged live", 1);
    tagged_live.tags = vec!["keep".into()];
    let cat = catalog(&[("bl-keep", tagged_live), ("bl-drop", task("Untagged", 2))]);
    let mut d = dead("bl-dkeep", "Tagged dead", 3);
    d.task.tags = vec!["keep".into()];
    let dead_set = [d, dead("bl-dother", "Untagged dead", 4)];
    let f = Flags { plain: true, reach: Reach::All, tags: vec!["keep".into()], ..Default::default() };
    let out = render(&cat, &dead_set, &f, &plain());
    assert_eq!(out, "ready    bl-keep  Tagged live\nclosed   bl-dkeep  Tagged dead\n");
}

#[test]
fn a_text_filter_searches_the_live_set() {
    let cat = catalog(&[("bl-1", task("Refactor auth", 1)), ("bl-2", task("Add caching", 2))]);
    let f = Flags { plain: true, target: Some("auth".into()), ..Default::default() };
    let out = render(&cat, &[], &f, &plain());
    assert_eq!(out, "ready    bl-1  Refactor auth\n");
}

#[test]
fn a_dead_ball_date_window_reads_its_deletion_date() {
    // The dead ball was created at 1 but retired_at = created + 1 = 2; a window
    // that excludes both created and retired drops it.
    let dead_set = [dead("bl-d", "Dead", 1)];
    let in_win = Flags { plain: true, reach: Reach::Dead, since: Some(2), until: Some(2 + 86_399), ..Default::default() };
    assert!(render(&catalog(&[]), &dead_set, &in_win, &plain()).contains("bl-d"));
    let out_win = Flags { plain: true, reach: Reach::Dead, since: Some(100), ..Default::default() };
    assert!(render(&catalog(&[]), &dead_set, &out_win, &plain()).is_empty());
}

#[test]
fn a_claimant_filter_narrows_both_live_and_dead() {
    // `-s closed --claimant X` = "what did X deliver": exact-match over the
    // stored `claimant`, uniform across the live and reconstructed-dead sets.
    let s = git_store();
    let mut live = task("Live held", 1);
    live.claimant = Some("alice".into());
    s.claim("bl-a", &live, NOW - 60); // 1m ago
    s.create("bl-b", &task("Unheld", 2), 2);
    let cat = Catalog::load(s.dir()).unwrap();
    let mut da = dead("bl-d", "Delivered by alice", 3);
    da.task.claimant = Some("alice".into());
    let dead_set = [da, dead("bl-other", "By someone else", 4)];
    let f = Flags { plain: true, reach: Reach::All, claimant: Some("alice".into()), ..Default::default() };
    let out = render_list(&cat, &dead_set, &f, &plain(), s.dir(), NOW).unwrap();
    assert!(out.contains("bl-a") && out.contains("bl-d"), "alice's live + dead: {out}");
    assert!(!out.contains("bl-b") && !out.contains("bl-other"), "others dropped: {out}");
    // The live row carries the derived age; the dead one renders `@alice` bare.
    assert!(out.contains("@alice (1m)"), "live claim-age: {out}");
    assert!(out.contains("bl-d  Delivered by alice  @alice\n"), "dead is ageless: {out}");
}

#[test]
fn a_claimed_row_without_a_claim_commit_renders_the_bare_claimant() {
    // The `claimant` is hand-set at `create` (e.g. an import) with no `bl-op:
    // claim` behind it: the walk finds no time, so the row keeps `@who` with no
    // age suffix rather than invent one.
    let s = git_store();
    let mut held = task("Held", 1);
    held.claimant = Some("alice".into());
    s.create("bl-1", &held, 1);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = render_list(&cat, &[], &flags(false), &plain(), s.dir(), NOW).unwrap();
    assert_eq!(out, "claimed  bl-1  Held  @alice\n");
}
