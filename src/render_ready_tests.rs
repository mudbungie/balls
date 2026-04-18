//! Coverage-targeted tests for the ready-queue renderer.

use super::*;
use crate::display::Display;
use crate::task::{NewTaskOpts, Status, Task};

fn mk(id: &str, title: &str) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: title.into(),
            ..Default::default()
        },
        id.into(),
    );
    t.status = Status::Open;
    t
}

#[test]
fn render_empty_returns_empty_string() {
    let out = render(&[], &[], Display::plain(), "me");
    assert!(out.is_empty());
}

#[test]
fn row_includes_status_column_and_id_and_title() {
    let t = mk("bl-1", "Do thing");
    let all = vec![t.clone()];
    let refs = vec![&all[0]];
    let out = render(&refs, &all, Display::plain(), "me");
    assert!(out.contains("[ ] open"));
    assert!(out.contains("bl-1"));
    assert!(out.contains("Do thing"));
}

#[test]
fn row_includes_tags_when_present() {
    let mut t = mk("bl-1", "tagged");
    t.tags = vec!["api".into(), "auth".into()];
    let all = vec![t.clone()];
    let refs = vec![&all[0]];
    let out = render(&refs, &all, Display::plain(), "me");
    assert!(out.contains("api, auth"));
}

#[test]
fn row_omits_tags_when_absent() {
    let t = mk("bl-1", "plain");
    let all = vec![t.clone()];
    let refs = vec![&all[0]];
    let out = render(&refs, &all, Display::plain(), "me");
    // No trailing double-space followed by a bare word that would be a tag.
    assert!(out.trim_end().ends_with("plain"));
}

#[test]
fn parent_hint_appended_when_parent_present_ascii() {
    let parent = mk("bl-p", "Auth epic");
    let mut child = mk("bl-c", "Swap middleware");
    child.parent = Some("bl-p".into());
    let all = vec![parent, child.clone()];
    let refs = vec![&all[1]];
    let out = render(&refs, &all, Display::plain(), "me");
    assert!(out.contains("^ bl-p (Auth epic)"));
}

#[test]
fn parent_hint_uses_unicode_arrow_when_styled() {
    let parent = mk("bl-p", "Auth epic");
    let mut child = mk("bl-c", "Swap");
    child.parent = Some("bl-p".into());
    let all = vec![parent, child.clone()];
    let refs = vec![&all[1]];
    let out = render(&refs, &all, Display::styled(), "me");
    assert!(out.contains("↑ bl-p (Auth epic)"));
}

#[test]
fn parent_hint_omitted_when_no_parent() {
    let t = mk("bl-1", "orphan");
    let all = vec![t.clone()];
    let refs = vec![&all[0]];
    let out = render(&refs, &all, Display::plain(), "me");
    assert!(!out.contains("^ bl-"));
}

#[test]
fn parent_hint_omitted_when_parent_missing_from_set() {
    let mut t = mk("bl-1", "dangling");
    t.parent = Some("bl-ghost".into());
    let all = vec![t.clone()];
    let refs = vec![&all[0]];
    let out = render(&refs, &all, Display::plain(), "me");
    assert!(!out.contains("bl-ghost"));
}

#[test]
fn claimed_badge_rendered_when_identity_matches() {
    let mut t = mk("bl-1", "mine");
    t.claimed_by = Some("me".into());
    let all = vec![t.clone()];
    let refs = vec![&all[0]];
    let out = render(&refs, &all, Display::plain(), "me");
    assert!(out.contains('*'));
}

#[test]
fn no_claimed_badge_when_identity_differs() {
    let mut t = mk("bl-1", "theirs");
    t.claimed_by = Some("other".into());
    let all = vec![t.clone()];
    let refs = vec![&all[0]];
    let with_claim = render(&refs, &all, Display::plain(), "me");
    // Same task without claimed_by renders identically: the claimed
    // badge is only rendered when the identity matches.
    let t_bare = mk("bl-1", "theirs");
    let all2 = vec![t_bare.clone()];
    let refs2 = vec![&all2[0]];
    let without = render(&refs2, &all2, Display::plain(), "me");
    assert_eq!(with_claim, without);
}
