//! Tests for the read-verb core: catalog loading, the §3 status resolver, flag
//! parsing, the diffless dispatch, and the shared rendering helpers.

use super::test_support::{blocker, catalog, task};
use super::*;
use crate::edge::Edge;
use crate::layout::Xdg;
use crate::task::On;
use tempfile::TempDir;

/// A claimed ball.
fn claimed(title: &str, created: i64, by: &str) -> Task {
    Task { claimant: Some(by.into()), ..task(title, created) }
}

#[test]
fn an_absent_store_loads_an_empty_catalog() {
    let cat = catalog(&[]);
    assert!(cat.entries().is_empty());
    // Every id is resolved when nothing is live.
    assert!(cat.is_resolved("bl-anything"));
}

#[test]
fn the_catalog_loads_and_id_sorts_its_balls() {
    let cat = catalog(&[("bl-z", task("Z", 1)), ("bl-a", task("A", 2))]);
    let ids: Vec<&str> = cat.entries().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["bl-a", "bl-z"]);
}

#[test]
fn is_resolved_reflects_file_existence() {
    let cat = catalog(&[("bl-live", task("Live", 1))]);
    assert!(!cat.is_resolved("bl-live")); // present ⇒ unresolved
    assert!(cat.is_resolved("bl-gone")); // absent ⇒ resolved
}

#[test]
fn status_climbs_the_three_rung_ladder() {
    let mut blocked = task("Blocked", 1);
    blocked.blockers = vec![blocker("bl-dep", On::Claim)];
    let cat = catalog(&[
        ("bl-ready", task("Ready", 1)),
        ("bl-claimed", claimed("Claimed", 1, "me")),
        ("bl-blocked", blocked),
        ("bl-dep", task("Dep", 1)), // live ⇒ the claim-blocker is unresolved
    ]);
    assert_eq!(cat.status(cat.get("bl-ready").unwrap()), Status::Ready);
    assert_eq!(cat.status(cat.get("bl-claimed").unwrap()), Status::Claimed);
    assert_eq!(cat.status(cat.get("bl-blocked").unwrap()), Status::Blocked);
}

#[test]
fn get_finds_a_ball_or_none() {
    let cat = catalog(&[("bl-1", task("One", 1))]);
    assert!(cat.get("bl-1").is_some());
    assert!(cat.get("bl-404").is_none());
}

#[test]
fn parse_reads_the_two_flags_and_one_positional() {
    let f = parse(Verb::List, &["--json".into(), "--plain".into()]).unwrap();
    assert!(f.json && f.plain && f.target.is_none());
    let f = parse(Verb::Show, &["bl-1".into()]).unwrap();
    assert_eq!(f.target.as_deref(), Some("bl-1"));
    assert!(!f.json && !f.plain);
}

#[test]
fn parse_rejects_bad_input() {
    assert!(parse(Verb::List, &["--nope".into()]).is_err()); // unknown flag
    assert!(parse(Verb::List, &["a".into(), "b".into()]).is_err()); // two positionals
    assert!(parse(Verb::Show, &[]).is_err()); // show needs an id
}

/// An edge whose store is seeded with `tasks`.
fn edge_with(tmp: &TempDir, tasks: &[(&str, Task)]) -> Edge {
    let xdg = Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy()));
    let edge = Edge {
        xdg,
        invocation_path: tmp.path().join("proj"),
        default_actor: "t".into(),
        depth: 0,
        tracker_bin: None,
        color: false,
    };
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    for (id, t) in tasks {
        crate::taskfile::write_task(&store, id, t).unwrap();
    }
    edge
}

#[test]
fn run_dispatches_each_read_verb() {
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[("bl-1", task("One", 1))]);
    run(&edge, Verb::List, &[]).unwrap();
    run(&edge, Verb::Ready, &[]).unwrap();
    run(&edge, Verb::DepTree, &[]).unwrap();
    run(&edge, Verb::Show, &["bl-1".into()]).unwrap();
}

#[test]
fn run_errors_on_a_missing_ball_and_a_non_read_verb() {
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[]);
    assert!(run(&edge, Verb::Show, &["bl-x".into()]).is_err());
    assert!(run(&edge, Verb::Create, &[]).is_err()); // not a read verb
}

#[test]
fn the_badge_is_a_padded_word_in_plain_mode() {
    let plain = Style { plain: true };
    assert_eq!(plain.badge(Status::Ready), "ready   ");
    assert_eq!(plain.badge(Status::Blocked), "blocked ");
    assert_eq!(plain.badge(Status::Claimed), "claimed ");
}

#[test]
fn the_badge_is_a_coloured_glyph_in_rich_mode() {
    let rich = Style { plain: false };
    // Each carries an ANSI reset and the status' glyph.
    assert!(rich.badge(Status::Ready).contains('\u{25cf}'));
    assert!(rich.badge(Status::Claimed).contains('\u{25d1}'));
    assert!(rich.badge(Status::Blocked).contains('\u{2298}'));
    assert!(rich.badge(Status::Ready).ends_with("\u{1b}[0m"));
}

#[test]
fn the_status_and_op_words_are_stable_tokens() {
    assert_eq!(status_word(Status::Ready), "ready");
    assert_eq!(status_word(Status::Claimed), "claimed");
    assert_eq!(status_word(Status::Blocked), "blocked");
    assert_eq!(on_word(On::Claim), "claim");
    assert_eq!(on_word(On::Close), "close");
}

#[test]
fn task_json_carries_the_machine_contract_fields() {
    let mut t = task("Title", 0);
    t.priority = Some(3);
    t.tags = vec!["x".into()];
    t.blockers = vec![blocker("bl-b", On::Close)];
    let v = task_json("bl-1", &t, Status::Ready);
    assert_eq!(v["id"], "bl-1");
    assert_eq!(v["status"], "ready");
    assert_eq!(v["priority"], 3);
    assert_eq!(v["created"], "1970-01-01T00:00:00Z");
    assert_eq!(v["blockers"][0]["on"], "close");
}

#[test]
fn json_line_is_pretty_and_newline_terminated() {
    let line = json_line(&serde_json::json!({ "a": 1 }));
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"a\": 1"));
}
