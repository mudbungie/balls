//! Unit tests for [`crate::speculate_queue`] — fixture repos, explicit
//! taggerdates (threaded per-command, never via global env), no ambient
//! identity.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{dequeue, enqueue, entry, queue};

/// `git -C <repo> <args>` with pinned identity, asserting success.
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo with two commits and `work/a` + `work/b` branches on the first.
fn repo() -> (TempDir, PathBuf, String, String) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("f"), "one").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "c1"]);
    let c1 = git(&root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("f"), "two").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "c2"]);
    let c2 = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["branch", "work/a", &c1]);
    git(&root, &["branch", "work/b", &c1]);
    (tmp, root, c1, c2)
}

fn ids(entries: &[super::Entry]) -> Vec<(&str, bool)> {
    entries.iter().map(|e| (e.id.as_str(), e.sealed)).collect()
}

#[test]
fn order_is_taggerdate_and_reenqueue_requeues_at_bottom() {
    let (_tmp, root, c1, _) = repo();
    let tip = enqueue(&root, "a", Some("2026-01-01T10:00:00Z")).unwrap();
    assert_eq!(tip, c1, "enqueue reports the sealed tip");
    enqueue(&root, "b", Some("2026-01-01T11:00:00Z")).unwrap();
    assert_eq!(ids(&queue(&root).unwrap()), vec![("a", true), ("b", true)]);
    enqueue(&root, "a", Some("2026-01-01T12:00:00Z")).unwrap();
    assert_eq!(
        ids(&queue(&root).unwrap()),
        vec![("b", true), ("a", true)],
        "re-enqueue IS requeue at bottom"
    );
}

#[test]
fn same_second_ties_break_by_refname() {
    let (_tmp, root, _, _) = repo();
    enqueue(&root, "b", Some("2026-01-01T10:00:00Z")).unwrap();
    enqueue(&root, "a", Some("2026-01-01T10:00:00Z")).unwrap();
    assert_eq!(
        ids(&queue(&root).unwrap()),
        vec![("a", true), ("b", true)],
        "deterministic even when taggerdates collide"
    );
}

#[test]
fn a_moved_branch_is_unsealed_and_a_deleted_one_too() {
    let (_tmp, root, c1, c2) = repo();
    enqueue(&root, "a", Some("2026-01-01T10:00:00Z")).unwrap();
    enqueue(&root, "b", Some("2026-01-01T11:00:00Z")).unwrap();
    git(&root, &["branch", "-f", "work/a", &c2]);
    git(&root, &["branch", "-D", "work/b"]);
    let entries = queue(&root).unwrap();
    assert_eq!(ids(&entries), vec![("a", false), ("b", false)]);
    assert_eq!(entries[0].tip, c1, "the SEALED tip is reported, not the runaway one");
}

#[test]
fn dequeue_is_the_only_exit_and_absence_errors() {
    let (_tmp, root, _, _) = repo();
    enqueue(&root, "a", Some("2026-01-01T10:00:00Z")).unwrap();
    dequeue(&root, "a").unwrap();
    assert!(queue(&root).unwrap().is_empty());
    assert!(dequeue(&root, "a").is_err(), "dequeuing an absent entry is loud");
}

#[test]
fn enqueue_without_a_work_branch_is_refused() {
    let (_tmp, root, _, _) = repo();
    let err = enqueue(&root, "ghost", Some("2026-01-01T10:00:00Z")).unwrap_err();
    assert!(err.to_string().contains("rev-parse"), "names the failing act: {err}");
}

#[test]
fn empty_queue_reads_empty_and_unparseable_lines_are_loud() {
    let (_tmp, root, _, _) = repo();
    assert!(queue(&root).unwrap().is_empty());
    assert!(entry(&root, "nospace").is_err());
}
