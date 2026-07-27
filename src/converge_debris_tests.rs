//! Tests for §12.2 debris report (bl-18bf piece 2, bl-3e5e): `changes/<uuid>/`
//! worktrees, the retired `stealth.lock` hazard, and the landing's own
//! `.git/index.lock` (bl-3e89), all three REPORT ONLY — nothing under test here
//! is ever deleted by `debris` itself. Two of the three name something that may
//! belong to a LIVE op, so their advice is CONDITIONED rather than instructing
//! (bl-7f82 brought the change worktree into the `index_lock` voice).

use crate::conf;
use crate::converge;
use crate::layout::Xdg;
use crate::substrate;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// An XDG rooted in `tmp`, and the one clone bundle it names — enough to found
/// a landing and give `debris` a real `CloneDir` to read under.
fn clone(tmp: &TempDir) -> crate::layout::CloneDir {
    let xdg = Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy()));
    xdg.clone_dir(&tmp.path().join("proj"))
}

fn founded(tmp: &TempDir) -> (crate::layout::CloneDir, PathBuf) {
    let clone = clone(tmp);
    let xdg = Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy()));
    let landing = clone.landing();
    substrate::found_landing(&landing, &xdg, None, "tester").unwrap();
    (clone, landing)
}

#[test]
fn a_clean_checkout_reports_nothing() {
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    assert_eq!(converge::debris(&clone, &landing).unwrap(), Vec::<String>::new());
}

#[test]
fn an_absent_changes_dir_is_not_an_error() {
    // Nothing has ever run an op here — `changes/` was never created.
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    assert!(!clone.root().join("changes").exists());
    assert!(converge::debris(&clone, &landing).unwrap().is_empty());
}

#[test]
fn a_change_worktree_names_the_removal_command_conditioned_on_no_op_running() {
    // bl-7f82: the removal is still named — a real crash's debris must be
    // reported — but CONDITIONED, because prime cannot tell a crashed op's
    // worktree from a running one's (an op holds it for its whole run).
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    let orphan = clone.change("dead-uuid");
    fs::create_dir_all(&orphan).unwrap();
    let notes = converge::debris(&clone, &landing).unwrap();
    assert_eq!(
        notes,
        vec![format!(
            "change worktree {} (crash debris unless an op is running here right now — an op holds its change worktree for its whole run, a close for its whole gate): with none running, remove with `git worktree remove {}`",
            orphan.display(),
            orphan.display()
        )]
    );
    assert!(orphan.exists(), "the report deletes nothing");
}

#[test]
fn a_change_worktree_is_never_called_orphaned_or_told_to_remove_unconditionally() {
    // The bug bl-7f82 fixed was the CONFIDENCE, not the reporting: an agent
    // following the old unconditional advice deleted a LIVE op's worktree and
    // its seal died on a vanished cwd. Nothing in the line may assert that the
    // op is gone, and the removal may never stand un-hedged.
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    fs::create_dir_all(clone.change("maybe-live")).unwrap();
    let note = converge::debris(&clone, &landing).unwrap().remove(0);
    assert!(!note.contains("orphan"), "prime cannot prove orphanhood: {note}");
    assert!(!note.contains("teardown never ran"), "prime cannot prove the op concluded: {note}");
    let (hedge, advice) = note.split_once("): ").expect("the hedge precedes the advice");
    assert!(hedge.contains("unless an op is running here right now"), "the hedge names liveness: {hedge}");
    assert!(advice.starts_with("with none running, remove with "), "advice is conditioned: {advice}");
}

#[test]
fn two_orphan_change_worktrees_each_get_their_own_line() {
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    fs::create_dir_all(clone.change("a")).unwrap();
    fs::create_dir_all(clone.change("b")).unwrap();
    assert_eq!(converge::debris(&clone, &landing).unwrap().len(), 2);
}

#[test]
fn a_changes_path_that_is_not_a_directory_surfaces_its_io_error() {
    // Anything other than "absent" (NotFound) from the readdir propagates
    // instead of being swallowed — here a plain file squats where `changes/`
    // should be a directory, so `read_dir` refuses it.
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    fs::write(clone.root().join("changes"), "").unwrap();
    assert!(converge::debris(&clone, &landing).is_err());
}

#[test]
fn a_stealth_lock_with_stealth_undeclared_warns_without_claiming_a_publish() {
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    fs::write(clone.root().join("stealth.lock"), "").unwrap();
    let notes = converge::debris(&clone, &landing).unwrap();
    assert_eq!(
        notes,
        vec![format!(
            "{} is retired and unread by the remote ladder — declare stealth with `bl conf set task-remote none`, then delete the file",
            clone.root().join("stealth.lock").display()
        )]
    );
    assert!(!notes[0].to_lowercase().contains("publish"), "core cannot see whether a remote resolves — never claim one happened");
}

#[test]
fn a_stealth_lock_is_suppressed_once_the_sentinel_already_declares_stealth() {
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    fs::write(clone.root().join("stealth.lock"), "").unwrap();
    conf::declare_stealth(&landing, "tester").unwrap();
    assert!(converge::debris(&clone, &landing).unwrap().is_empty(), "operator re-declared — the file is inert cruft, stay silent");
}

#[test]
fn an_absent_stealth_lock_is_silent_even_when_stealth_is_undeclared() {
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    assert!(!clone.root().join("stealth.lock").exists());
    assert!(converge::debris(&clone, &landing).unwrap().is_empty());
}

#[test]
fn an_index_lock_in_the_landing_names_the_lock_and_the_removal() {
    // bl-3e89: git's own lock, left by an op killed mid-commit. Report only —
    // it may be LIVE, so the line hedges and hands over `rm`, never deletes.
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    let lock = landing.join(".git").join("index.lock");
    fs::write(&lock, "").unwrap();
    let notes = converge::debris(&clone, &landing).unwrap();
    assert_eq!(
        notes,
        vec![format!(
            "git index lock {} blocks every commit in this landing, founding's `git add -A` included (crash debris unless an op is running here right now): with none running, remove with `rm {}`",
            lock.display(),
            lock.display()
        )]
    );
    assert!(lock.exists(), "the report deletes nothing");
}

#[test]
fn an_absent_index_lock_is_silent() {
    let tmp = TempDir::new().unwrap();
    let (clone, landing) = founded(&tmp);
    assert!(!landing.join(".git").join("index.lock").exists());
    assert!(converge::debris(&clone, &landing).unwrap().is_empty());
}
