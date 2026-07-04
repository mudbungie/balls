//! Tests for `bl show <id>` — the full-ball field block, `--json` detail, and
//! the history fallthrough that reconstructs a dead ball.

use std::path::Path;

use super::*;
use crate::civil::iso8601;
use crate::reads::test_support::{blocker, catalog, git_store, task};
use crate::reads::{Flags, Style};
use crate::task::{On, Task};

/// A fixed render clock the derived claim-age is measured against.
const NOW: i64 = 2_000_000_000;

fn flags(json: bool, target: &str) -> Flags {
    Flags { json, plain: true, target: Some(target.into()), ..Default::default() }
}

fn plain() -> Style {
    Style { plain: true }
}

/// A store path that resolves no live ball — the live catalog short-circuits a
/// live hit, so the unused path is never walked for those tests.
fn nostore() -> &'static Path {
    Path::new("/balls-no-such-store")
}

/// A fully-populated ball: every optional field, a blocker, and a body.
fn rich_task() -> Task {
    let mut t = task("Refactor", 0);
    t.claimant = Some("alice".into());
    t.priority = Some(2);
    t.parent = Some("bl-root".into());
    t.tags = vec!["infra".into(), "refactor".into()];
    t.blockers = vec![blocker("bl-dep", On::Claim), blocker("bl-gate", On::Close)];
    t.body = "Some body text.".into();
    t
}

#[test]
fn show_renders_every_present_field_the_body_and_the_journal() {
    let s = git_store();
    s.create("bl-1", &rich_task(), 100).create("bl-kid", &child_of("bl-1"), 200);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-1"), &plain(), "", NOW).unwrap();
    for fragment in [
        "bl-1  Refactor",
        "claimant alice",
        "priority 2",
        "parent   bl-root",
        "tags     infra, refactor",
        "  blockers\n    bl-dep (on claim)\n    bl-gate (on close)\n",
        "  children\n    ready    bl-kid",
        "Some body text.",
    ] {
        assert!(out.contains(fragment), "missing {fragment:?} in:\n{out}");
    }
    // The journal (bl-0e16) folds in one paragraph AFTER the body — the
    // ball's own store history, oldest-first.
    assert!(
        out.ends_with("Some body text.\n\n  journal\n    1970-01-01T00:01:40Z  create   t\n"),
        "journal after the body:\n{out}"
    );
}

/// A child pointing at `parent`.
fn child_of(parent: &str) -> Task {
    Task { parent: Some(parent.into()), ..task("Kid", 1) }
}

#[test]
fn show_omits_absent_optional_fields_blockers_children_body_and_journal() {
    // The bare ball's file is live but UNCOMMITTED: no store history yet, so
    // the journal section is absent too, not an empty header.
    let s = git_store();
    s.create("bl-other", &task("Other", 5), 5);
    crate::taskfile::write_task(s.dir(), "bl-bare", &task("Bare", 0)).unwrap();
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-bare"), &plain(), "", NOW).unwrap();
    assert!(out.contains("status   ready"));
    for absent in ["claimant", "priority", "parent", "tags", "blockers", "children", "journal"] {
        assert!(!out.contains(absent), "unexpected {absent:?} in:\n{out}");
    }
    // No body ⇒ no trailing blank line + text.
    assert!(out.ends_with("updated  1970-01-01T00:00:00Z\n"));
}

#[test]
fn show_folds_the_journal_after_the_field_block_when_there_is_no_body() {
    let s = git_store();
    s.create("bl-1", &task("Bare", 100), 100);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-1"), &plain(), "", NOW).unwrap();
    assert!(
        out.ends_with("updated  1970-01-01T00:01:40Z\n\n  journal\n    1970-01-01T00:01:40Z  create   t\n"),
        "one blank line between the block and the journal:\n{out}"
    );
}

#[test]
fn show_json_is_the_bedrock_record_total_with_no_derived_fields() {
    let cat = catalog(&[("bl-1", rich_task()), ("bl-kid", child_of("bl-1"))]);
    // nostore() doubles as the journal guard: `--json` never walks the store
    // for history (bl-0e16) — a walk against this path would error.
    let out = dispatch(nostore(), &cat, &flags(true, "bl-1"), &plain(), "", NOW).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    // Stored frontmatter round-trips; the i64 timestamp is literal.
    assert_eq!(v["id"], "bl-1");
    assert_eq!(v["parent"], "bl-root");
    assert!(v["created"].is_i64());
    // The record is TOTAL (bl-e614): the stored body rides too, so `bl import`
    // writes the whole ball back from it.
    assert_eq!(v["body"], "Some body text.");
    // Derived fields stay absent — the human render owns them.
    for derived in ["children", "status"] {
        assert!(v.get(derived).is_none(), "bedrock must omit {derived}");
    }
}

#[test]
fn show_folds_the_read_dispatch_lines_into_the_field_block_not_json() {
    // §6 read dispatch: a wired plugin's captured stdout (the delivery worktree
    // line, §11) is folded verbatim between the field block and the body…
    let s = git_store();
    s.create("bl-1", &rich_task(), 100);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-1"), &plain(), "  worktree /wt/bl-1\n", NOW).unwrap();
    assert!(out.contains("  worktree /wt/bl-1\n\nSome body text."), "fold precedes the body:\n{out}");
    // …and `--json` stays the bedrock store mirror whatever the caller passes
    // (reads::run never dispatches for it; this guards the render half).
    let json = dispatch(s.dir(), &cat, &flags(true, "bl-1"), &plain(), "  worktree /wt/bl-1\n", NOW).unwrap();
    assert!(!json.contains("worktree"));
}

#[test]
fn show_errors_when_the_id_is_unknown() {
    let cat = catalog(&[("bl-1", task("One", 0))]);
    assert!(dispatch(nostore(), &cat, &flags(false, "bl-404"), &plain(), "", NOW).is_err());
}

#[test]
fn show_falls_through_to_history_for_a_dead_ball() {
    // The id is absent from the live catalog, so dispatch walks the store.
    let s = git_store();
    let mut t = task("Closed work", 100);
    t.priority = Some(2);
    t.body = "what it did".into();
    s.create("bl-dead", &t, 100).note("bl-dead", &t, "midway note", 300);
    s.retire("bl-dead", "close", 500);
    let cat = Catalog::load(s.dir()).unwrap(); // bl-dead is NOT live

    let out = dispatch(s.dir(), &cat, &flags(false, "bl-dead"), &plain(), "", NOW).unwrap();
    assert!(out.contains("closed   bl-dead  Closed work")); // retirement badge
    assert!(out.contains("status   closed"));
    assert!(out.contains("retired  1970-01-01T00:08:20Z")); // 500s past the epoch
    assert!(out.contains("priority 2"));
    assert!(out.contains("\nwhat it did\n")); // reconstructed body
    // A closed id renders its journal too (bl-0e16) — the same walk, its
    // deletion the last entry, the `-m` note under its update line.
    assert!(
        out.ends_with(
            "  journal\n    1970-01-01T00:01:40Z  create   t\n    1970-01-01T00:05:00Z  update   t\n\
             \x20     midway note\n    1970-01-01T00:08:20Z  close    t\n"
        ),
        "dead-ball journal:\n{out}"
    );
}

#[test]
fn show_renders_a_dropped_ball_as_closed() {
    // A `drop` retirement projects as `closed` like any other dead ball — the
    // verb survives only in git history (`bl-op: drop`), not as a status word.
    // The journal IS that history rendered, so the op shows there and only there.
    let s = git_store();
    s.create("bl-gone", &task("Abandoned", 1), 1).retire("bl-gone", "drop", 9);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-gone"), &plain(), "", NOW).unwrap();
    assert!(out.contains("closed   bl-gone  Abandoned"));
    assert!(out.contains("status   closed"));
    assert!(!out.contains("dropped"));
    assert!(out.contains("  journal\n") && out.contains("drop     t\n"), "{out}");
}

#[test]
fn show_dead_json_is_the_reconstructed_bedrock_record() {
    let s = git_store();
    s.create("bl-d", &task("Dead", 1_700_000_000), 1).retire("bl-d", "close", 9);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(true, "bl-d"), &plain(), "", NOW).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["id"], "bl-d");
    assert_eq!(v["created"], 1_700_000_000); // literal stored i64, no derived retirement
    assert!(v.get("status").is_none());
}

#[test]
fn show_surfaces_a_corrupt_live_ball_and_never_resurrects_its_dead_past() {
    // bl-528c: the id's FILE exists but no longer parses. Show must surface
    // the parse error — not "no such ball", and above all not the history
    // fallthrough, which would render the id's stale dead incarnation.
    let s = git_store();
    s.create("bl-r", &task("First life", 1), 1).retire("bl-r", "close", 2);
    std::fs::create_dir_all(s.dir().join("tasks")).unwrap();
    std::fs::write(s.dir().join("tasks/bl-r.md"), "+++\ntitle = 1\n+++\n").unwrap();
    let cat = Catalog::load(s.dir()).unwrap();
    let err = dispatch(s.dir(), &cat, &flags(false, "bl-r"), &plain(), "", NOW).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("tasks/bl-r.md"), "names the file: {msg}");
    assert!(msg.contains("invalid frontmatter"), "carries the parse error: {msg}");
    assert!(!msg.contains("First life"), "no stale resurrection: {msg}");
}

#[test]
fn show_errors_with_no_such_ball_when_history_has_nothing() {
    // A real git store where the id was never created — resolve_dead is None,
    // so dispatch returns the "no such ball" error (not a git failure).
    let s = git_store();
    s.create("bl-other", &task("Other", 1), 1);
    let cat = Catalog::load(s.dir()).unwrap();
    let err = dispatch(s.dir(), &cat, &flags(false, "bl-ghost"), &plain(), "", NOW).unwrap_err();
    assert!(err.to_string().contains("no such ball"));
}

#[test]
fn show_renders_the_derived_claim_age_line_under_the_claimant() {
    // A live claimed ball folds a derived `claimed <ISO> (<age> ago)` line
    // under its `claimant` — human-only, from the newest `bl-op: claim` commit.
    let s = git_store();
    let claim_ts = NOW - 3 * 3_600; // 3h ago
    s.create("bl-1", &rich_task(), 100).claim("bl-1", &rich_task(), claim_ts);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-1"), &plain(), "", NOW).unwrap();
    assert!(out.contains("claimant alice"), "who: {out}");
    assert!(out.contains(&format!("claimed  {} (3h ago)", iso8601(claim_ts))), "derived age line: {out}");
}

#[test]
fn show_omits_the_claim_age_line_when_the_claimant_has_no_claim_commit() {
    // Claimant hand-set at create (an import) with no `bl-op: claim` behind it:
    // the `claimant` field still shows, but no derived `claimed` line does.
    let s = git_store();
    let mut t = task("Held", 1);
    t.claimant = Some("alice".into());
    s.create("bl-1", &t, 1);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-1"), &plain(), "", NOW).unwrap();
    assert!(out.contains("claimant alice"));
    assert!(!out.contains("ago)"), "no derived claim line: {out}");
}
