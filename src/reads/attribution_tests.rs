//! Tests for the §9 comment byline (bl-236c) — the blame-derived attribution
//! human `bl show` hangs under each `comment`-op region of a body. The
//! DEGRADATION paths are most of the work: everything that is not a comment
//! renders bare, and so does everything blame cannot speak for.

use super::*;
use crate::reads::test_support::{git_store, task};
use crate::task::Task;

/// The plain style every assertion below reads: no colour, so the byline is
/// exactly its `  <ISO>  <actor>` text.
fn plain() -> Style {
    Style { plain: true }
}

/// A ball whose body is `text`.
fn bodied(text: &str) -> Task {
    Task { body: text.into(), ..task("A ball", 100) }
}

/// The body `bl comment` produces when `text` is appended to `body` — the seam
/// [`crate::mutate_build::appended_body`] writes (blank line, rule, blank line),
/// composed here so a fixture owns the exact bytes each commit adds.
fn appended(body: &str, text: &str) -> String {
    format!("{}\n\n---\n\n{}\n", body.trim_end(), text.trim_end())
}

#[test]
fn each_comment_carries_its_byline_and_the_original_body_stays_bare() {
    let s = git_store();
    let t = bodied("The original body, as filed.\n");
    let one = appended(&t.body, "single-clone runs pass; needs a second clone");
    let two = appended(&one, "second comment\nspanning two lines");
    s.create("bl-1", &t, 100);
    s.edit("bl-1", &t, "comment", &one, 200);
    s.edit("bl-1", &t, "comment", &two, 300);
    let out = annotate(s.dir(), "HEAD", "bl-1", &two, &plain()).unwrap();
    assert_eq!(
        out,
        "The original body, as filed.\n\
         \n\
         ---\n\
         \n\
         single-clone runs pass; needs a second clone\n\
         \x20 1970-01-01T00:03:20Z  t\n\
         \n\
         ---\n\
         \n\
         second comment\n\
         spanning two lines\n\
         \x20 1970-01-01T00:05:00Z  t\n"
    );
}

#[test]
fn a_create_body_and_a_body_rewrite_are_left_bare() {
    // Neither is a note — the living document gets no byline, whoever last
    // wrote it. The rewrite here replaces the WHOLE body under one `update`,
    // so it stands for `--body` and `--edit` alike.
    let s = git_store();
    let t = bodied("as filed\n");
    s.create("bl-1", &t, 100);
    assert_eq!(annotate(s.dir(), "HEAD", "bl-1", &t.body, &plain()).unwrap(), "as filed\n");
    let rewritten = "rewritten wholesale\nover two lines\n";
    s.edit("bl-1", &t, "update", rewritten, 200);
    assert_eq!(annotate(s.dir(), "HEAD", "bl-1", rewritten, &plain()).unwrap(), rewritten);
}

#[test]
fn an_imported_ball_collapses_onto_its_import_commit_and_renders_bare() {
    // bl-e614: `import` reproduces a whole ball under ONE commit of its own, so
    // blame attributes every line to the importer — the rule and the comment
    // text that rode along in the body included. Honest and unrepaired: the
    // byline is a `comment`-op fact and no comment op ran in this store.
    let s = git_store();
    let t = bodied(&appended("as filed", "a real comment, in the source store"));
    s.edit("bl-1", &t, "import", &t.body, 100);
    let out = annotate(s.dir(), "HEAD", "bl-1", &t.body, &plain()).unwrap();
    assert_eq!(out, t.body, "every line is the importer's, so none is a comment");
}

#[test]
fn a_closed_ball_is_blamed_where_its_bytes_last_lived() {
    // A dead ball's body is reconstructed from the deletion's PARENT, so its
    // bylines derive at that same revision — HEAD no longer holds the file, and
    // blaming HEAD would say nothing at all.
    let s = git_store();
    let t = bodied("what it did\n");
    let commented = appended(&t.body, "and how it went");
    s.create("bl-1", &t, 100);
    s.edit("bl-1", &t, "comment", &commented, 200);
    s.retire("bl-1", "close", 300);
    let dead = crate::reads::resolve_dead(s.dir(), "bl-1").unwrap().unwrap();
    let out = annotate(s.dir(), &dead.rev, "bl-1", &dead.task.body, &plain()).unwrap();
    assert_eq!(out, "what it did\n\n---\n\nand how it went\n  1970-01-01T00:03:20Z  t\n");
}

#[test]
fn an_empty_body_makes_no_blame_call_at_all() {
    // The store path is not walkable, so ANY git call here would error — an
    // empty body must make none. Nothing to attribute, nothing paid.
    let nostore = std::path::Path::new("/balls-no-such-store");
    assert_eq!(annotate(nostore, "HEAD", "bl-1", "", &plain()).unwrap(), "");
}

#[test]
fn a_body_git_cannot_blame_renders_bare_rather_than_erroring() {
    // The ball's file was written but never sealed: blame has no answer for the
    // path, so the body passes through untouched. Blame is the ONE input —
    // nothing said is nothing rendered, never an error and never a warning.
    let s = git_store();
    s.create("bl-other", &task("Other", 1), 1);
    let t = bodied("written but never sealed\n");
    crate::taskfile::write_task(s.dir(), "bl-new", &t).unwrap();
    let out = annotate(s.dir(), "HEAD", "bl-new", &t.body, &plain()).unwrap();
    assert_eq!(out, "written but never sealed\n");
}

#[test]
fn a_body_with_no_trailing_newline_still_gets_its_byline_on_its_own_line() {
    // A body need not end in a newline, and the file's last line can be the
    // comment's own. The byline is a line whatever the text above it did.
    let s = git_store();
    let t = bodied("as filed\n");
    let unterminated = "as filed\n\n---\n\nunterminated";
    s.create("bl-1", &t, 100);
    s.edit("bl-1", &t, "comment", unterminated, 200);
    let out = annotate(s.dir(), "HEAD", "bl-1", unterminated, &plain()).unwrap();
    assert_eq!(out, "as filed\n\n---\n\nunterminated\n  1970-01-01T00:03:20Z  t\n");
}

#[test]
fn the_rich_byline_is_dim_and_the_plain_one_is_the_same_line_bare() {
    // `--plain` degrades by dropping COLOUR alone: same content, no escape
    // sequence, and no glyph to lose in the first place.
    assert_eq!(Style { plain: false }.byline(200, "alice"), "\u{1b}[90m  1970-01-01T00:03:20Z  alice\u{1b}[0m");
    assert_eq!(plain().byline(200, "alice"), "  1970-01-01T00:03:20Z  alice");
}
