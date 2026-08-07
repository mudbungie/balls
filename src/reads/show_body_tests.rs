//! What the human `show` render does with the BODY — a nested sibling of
//! [`super`] (the `show` render tests) so that file stays under the 300-line
//! cap, inheriting every fixture there (`NOW`, `flags`, `plain`, `nostore`,
//! `rich_task`, `catalog`, `git_store`, `task`, …) through `use super::*`.
//!
//! Three facts, all about the same seam: the §6 read-dispatch fold that
//! precedes the body, the §9 comment bylines derived over it (bl-236c), and the
//! §16 `--legacy` stand-down that renders it bare.

use super::*;

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
fn show_hangs_a_comment_byline_under_each_comment_and_json_carries_none() {
    // bl-236c: the human render folds a derived byline under every `comment`-op
    // region of the body; the ORIGINAL body stays bare, and the body bytes are
    // untouched — the rule is still never read, only rendered through.
    let s = git_store();
    let mut t = task("A ball", 100);
    t.body = "The original body, as filed.\n".into();
    let commented = "The original body, as filed.\n\n---\n\nsingle-clone runs pass\n";
    s.create("bl-1", &t, 100);
    s.edit("bl-1", &t, "comment", commented, 200);
    let cat = Catalog::load(s.dir()).unwrap();
    let out = dispatch(s.dir(), &cat, &flags(false, "bl-1"), &plain(), "", NOW).unwrap();
    assert!(
        out.contains("\nThe original body, as filed.\n\n---\n\nsingle-clone runs pass\n  1970-01-01T00:03:20Z  t\n"),
        "byline under the comment, original bare:\n{out}"
    );
    // Bedrock is the stored file and nothing else — no byline, no actor, and it
    // never pays the blame (§3: derived means human-only).
    let json = dispatch(s.dir(), &cat, &flags(true, "bl-1"), &plain(), "", NOW).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["body"], commented, "the body verbatim, unannotated");
    assert!(!json.contains("1970-01-01"), "no derived date anywhere in bedrock:\n{json}");
}

#[test]
fn show_renders_a_legacy_ball_bare_and_walks_this_store_for_nothing() {
    // §16: a `--legacy` ball's history lives on the legacy ref, not this store,
    // so every store-derived read stands down together — the journal, the
    // claim-age line, and the comment bylines. nostore() is the proof: any walk
    // against it errors.
    let cat = catalog(&[("bl-1", rich_task())]);
    let legacy = Flags { legacy: Some("balls/tasks:.balls/tasks".into()), ..flags(false, "bl-1") };
    let out = dispatch(nostore(), &cat, &legacy, &plain(), "", NOW).unwrap();
    assert!(out.ends_with("Some body text."), "the stored body, bare:\n{out}");
    assert!(!out.contains("journal") && !out.contains("ago)"), "no store-derived line:\n{out}");
}
