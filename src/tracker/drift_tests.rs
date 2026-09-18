//! Tests for the drift render (bl-439d): the mark, the three surfaces, and
//! the honesty stamp — ahead is exact, behind is as fresh as the last fetch.

use super::*;
use crate::tracker::fixtures::{binding, checkout, commit, env_top, remote_with_branch, store_clone, tip, BRANCH};
use crate::tracker::remote_ops::{push, sync};
use tempfile::TempDir;

#[test]
fn no_mark_means_unknown_everywhere_and_a_quiet_post() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote); // a clone: nothing fetched or pushed by the TRACKER yet
    assert!(read(&store, None).is_none());
    assert!(op_line(&store, Some("bl-1")).is_none());
    assert!(show_line(&store, "bl-1", 0).contains("unknown — never synced"));
    assert!(list_header(&store, "r", BRANCH, 0).starts_with("store: publication unknown"));
}

#[test]
fn a_push_marks_head_and_a_fetch_marks_the_remote_tip() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    commit(&store, "tasks/bl-0001.md", "one");
    push(&binding(Some(&remote), &store), &env_top()).unwrap();
    let d = read(&store, None).unwrap();
    assert_eq!((d.ahead, d.behind), (0, 0), "a successful push means the mark IS head");

    // Another writer moves the remote; a sync fetches (mark = remote tip) and
    // rebases (a plain ff here), so the store is current again afterwards.
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "tasks/bl-0002.md", "two");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    sync(&binding(Some(&remote), &store), &env_top()).unwrap();
    let d = read(&store, None).unwrap();
    assert_eq!((d.ahead, d.behind), (0, 0));
    assert_eq!(tip(&store, "HEAD"), tip(&remote, BRANCH));
}

#[test]
fn ahead_is_per_ball_when_asked_and_the_post_line_counts_only_that_ball() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    push(&binding(Some(&remote), &store), &env_top()).unwrap(); // mark = head, current
    commit(&store, "tasks/bl-aaaa.md", "a1");
    commit(&store, "tasks/bl-aaaa.md", "a2");
    commit(&store, "tasks/bl-bbbb.md", "b1");
    assert_eq!(read(&store, None).unwrap().ahead, 3);
    assert_eq!(read(&store, Some("bl-aaaa")).unwrap().ahead, 2);
    assert_eq!(op_line(&store, Some("bl-aaaa")).as_deref(), Some("bl-aaaa: 2 seals unpublished"));
    assert_eq!(op_line(&store, Some("bl-bbbb")).as_deref(), Some("bl-bbbb: 1 seal unpublished"));
    assert_eq!(op_line(&store, None).as_deref(), Some("store: 3 seals unpublished"));
    assert!(op_line(&store, Some("bl-cccc")).is_none(), "a ball with nothing unpublished says nothing");
}

#[test]
fn behind_reads_against_the_mark_and_carries_the_fetch_age() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    // Simulate "fetched, not yet integrated": mark the remote's future tip
    // without moving the branch (what a fetch leaves behind before its rebase).
    let other = checkout(tmp.path(), &remote, "other");
    let theirs = commit(&other, "tasks/bl-cafe.md", "theirs");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    git(&store, &["fetch", "-q", "origin", BRANCH]).unwrap();
    mark(&store, &theirs);
    commit(&store, "tasks/bl-face.md", "mine");
    let now = read(&store, None).unwrap().fetched + 2 * 3_600;
    let line = show_line(&store, "bl-face", now);
    assert!(line.starts_with("  published 1 seal unpublished (last fetch 2h ago)"), "{line}");
    let line = show_line(&store, "bl-cafe", now);
    assert!(line.starts_with("  published 1 seal to sync (last fetch 2h ago)"), "{line}");
    let header = list_header(&store, "hub", BRANCH, now);
    assert_eq!(header, format!("store: 1 ahead, 1 behind `hub` {BRANCH} (last fetch 2h ago)\n"));
    // Both at once, per ball: an edit here AND theirs over there on one file.
    commit(&store, "tasks/bl-cafe.md", "mine too");
    assert!(show_line(&store, "bl-cafe", now).contains("1 seal unpublished, 1 seal to sync"));
}

#[test]
fn render_is_the_read_dispatch_and_stays_silent_in_stealth() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    push(&binding(Some(&remote), &store), &env_top()).unwrap();
    let mut out = Vec::new();
    render("show", &binding(Some(&remote), &store), Some("bl-1"), &mut out).unwrap();
    assert!(String::from_utf8(out).unwrap().starts_with("  published current (last fetch 0m ago)"));
    let mut out = Vec::new();
    render("list", &binding(Some(&remote), &store), None, &mut out).unwrap();
    assert!(String::from_utf8(out).unwrap().starts_with("store: 0 ahead, 0 behind `"));
    let mut out = Vec::new();
    render("show", &binding(Some(&remote), &store), None, &mut out).unwrap(); // a show with no id: nothing to say
    render("show", &binding(None, &store), Some("bl-1"), &mut out).unwrap(); // stealth: nothing
    assert!(out.is_empty());
}
