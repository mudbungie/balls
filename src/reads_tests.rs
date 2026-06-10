//! Tests for the read-verb core: catalog loading, the §3 status resolver, flag
//! parsing, and the shared rendering helpers. The `run` lifecycle (narration +
//! the §6 read dispatch) is covered by the sibling [`super::run_tests`].

use super::test_support::{blocker, catalog, task};
use super::*;
use crate::task::On;

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
    assert!(f.json && f.plain && f.target.is_none() && f.status.is_none());
    let f = parse(Verb::Show, &["bl-1".into()]).unwrap();
    assert_eq!(f.target.as_deref(), Some("bl-1"));
    assert!(!f.json && !f.plain);
}

#[test]
fn parse_reads_the_list_status_filter() {
    let f = parse(Verb::List, &["--status".into(), "blocked".into()]).unwrap();
    assert_eq!(f.status, Some(Status::Blocked));
    // `-s` is the short alias for `--status`.
    let f = parse(Verb::List, &["-s".into(), "claimed".into()]).unwrap();
    assert_eq!(f.status, Some(Status::Claimed));
    // The `closed` rung carries no live predicate — it only steers the reach.
    let f = parse(Verb::List, &["-s".into(), "closed".into()]).unwrap();
    assert!(f.status.is_none() && f.reach == Reach::Dead);
}

#[test]
fn parse_rejects_bad_input() {
    assert!(parse(Verb::List, &["--nope".into()]).is_err()); // unknown flag
    assert!(parse(Verb::Show, &["-h".into()]).is_err()); // single-dash unknown is a flag, not an id
    assert!(parse(Verb::List, &["a".into(), "b".into()]).is_err()); // two positionals
    assert!(parse(Verb::Show, &[]).is_err()); // show needs an id
    assert!(parse(Verb::List, &["--status".into(), "bogus".into()]).is_err()); // bad rung
    assert!(parse(Verb::List, &["--status".into()]).is_err()); // missing value
    assert!(parse(Verb::Show, &["--status".into(), "ready".into()]).is_err()); // list-only flag
}

#[test]
fn parse_reads_the_legacy_shim_flag_on_both_reads() {
    // §16: `--legacy[=REF]` rides list (the migration preview) AND show.
    let f = parse(Verb::List, &["--legacy".into()]).unwrap();
    assert_eq!(f.legacy.as_deref(), Some(legacy::DEFAULT_SPEC));
    let f = parse(Verb::Show, &["bl-1".into(), "--legacy=v1:old".into()]).unwrap();
    assert_eq!(f.legacy.as_deref(), Some("v1:old"));
    // The legacy store holds the LIVE set alone — a dead-set reach contradicts.
    assert!(parse(Verb::List, &["--legacy".into(), "--all".into()]).is_err());
    assert!(parse(Verb::List, &["--legacy".into(), "-s".into(), "closed".into()]).is_err());
}

/// Parse a `bl list` argv from string slices.
fn list(args: &[&str]) -> io::Result<Flags> {
    parse(Verb::List, &args.iter().map(ToString::to_string).collect::<Vec<_>>())
}

#[test]
fn parse_reads_the_history_reach_flags() {
    assert_eq!(list(&[]).unwrap().reach, Reach::Live); // default
    // `--status closed` INFERS the dead reach — no separate `--closed` flag.
    assert_eq!(list(&["--status", "closed"]).unwrap().reach, Reach::Dead);
    assert_eq!(list(&["-s", "closed"]).unwrap().reach, Reach::Dead); // -s alias
    assert_eq!(list(&["--all"]).unwrap().reach, Reach::All);
    // A live rung leaves the reach at its live default.
    assert_eq!(list(&["-s", "ready"]).unwrap().reach, Reach::Live);
}

#[test]
fn parse_rejects_two_reach_flags() {
    assert!(list(&["--status", "closed", "--all"]).is_err());
    assert!(list(&["--all", "--status", "closed"]).is_err());
    // The reach axis is list-only.
    assert!(parse(Verb::Show, &["bl-1".into(), "--all".into()]).is_err());
}

#[test]
fn parse_collects_repeatable_tags_and_the_text_needle() {
    let f = list(&["--tag", "infra", "--tag", "api", "find me"]).unwrap();
    assert_eq!(f.tags, ["infra", "api"]);
    assert_eq!(f.target.as_deref(), Some("find me"));
}

#[test]
fn parse_reads_the_date_window_with_an_inclusive_until() {
    let f = list(&["--since", "2026-01-01", "--until", "2026-01-01"]).unwrap();
    let start = crate::civil::start_of_day("2026-01-01").unwrap();
    assert_eq!(f.since, Some(start));
    assert_eq!(f.until, Some(start + 86_399)); // whole day inclusive
}

#[test]
fn parse_rejects_bad_dates_and_missing_filter_values() {
    assert!(list(&["--since", "nope"]).is_err()); // unparseable date
    assert!(list(&["--until", "2026-13-01"]).is_err()); // month out of range
    assert!(list(&["--tag"]).is_err()); // missing value
    assert!(list(&["--since"]).is_err()); // missing value
    assert!(list(&["--until"]).is_err()); // missing value
}

#[test]
fn reach_predicates_split_live_and_dead() {
    assert!(Reach::Live.live() && !Reach::Live.dead());
    assert!(!Reach::Dead.live() && Reach::Dead.dead());
    assert!(Reach::All.live() && Reach::All.dead());
}

#[test]
fn every_retirement_reads_as_closed() {
    // A `drop` is a close without delivery; both collapse to `closed` in
    // projection. The badge takes no retirement arg — there is nothing to split.
    let plain = Style { plain: true };
    assert_eq!(plain.retired_badge(), "closed  ");
    let rich = Style { plain: false };
    assert!(rich.retired_badge().contains('\u{2713}')); // ✓
    assert!(!rich.retired_badge().contains('\u{2717}')); // never a drop ✗
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
    let mut t = task("Title", 1_700_000_000);
    t.priority = Some(3);
    t.tags = vec!["x".into()];
    t.blockers = vec![blocker("bl-b", On::Close)];
    let v = task_json("bl-1", &t);
    assert_eq!(v["id"], "bl-1");
    assert_eq!(v["priority"], 3);
    // The LITERAL stored i64, not an ISO string — the lossless export (§3).
    assert_eq!(v["created"], 1_700_000_000);
    assert!(v["created"].is_i64());
    assert_eq!(v["blockers"][0]["on"], "close");
    // Bedrock carries NO derived field — the status ladder is human-only (§9).
    assert!(v.get("status").is_none());
}

#[test]
fn task_json_round_trips_preserved_extras() {
    // Bedrock is LOSSLESS: a preserved (unknown) frontmatter key — a team's
    // `state:`, a plugin's namespaced field (§3 seam) — survives into `--json`,
    // not just the canonical fields (§3, bl-d074).
    let mut t = task("Title", 1_700_000_000);
    t.extra.insert("review-by".into(), "bob".into());
    t.extra.insert("state".into(), "in-review".into());
    let v = task_json("bl-1", &t);
    assert_eq!(v["review-by"], "bob");
    assert_eq!(v["state"], "in-review");
    // ...and the canonical fields still ride alongside, uncollided.
    assert_eq!(v["id"], "bl-1");
    assert_eq!(v["title"], "Title");
}

#[test]
fn json_line_is_pretty_and_newline_terminated() {
    let line = json_line(&serde_json::json!({ "a": 1 }));
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"a\": 1"));
}
