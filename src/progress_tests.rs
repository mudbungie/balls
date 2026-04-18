//! Coverage tests for the epic progress bar module.

use super::*;
use crate::display::Display;
use crate::task::{ArchivedChild, NewTaskOpts, Status, Task};
use chrono::Utc;

fn mk(id: &str, status: Status, parent: Option<&str>) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: id.into(),
            parent: parent.map(String::from),
            ..Default::default()
        },
        id.into(),
    );
    t.status = status;
    t
}

#[test]
fn counts_empty_parent_is_zero_zero() {
    let p = mk("p", Status::Open, None);
    assert_eq!(counts(&[p], "p"), (0, 0));
}

#[test]
fn counts_unknown_parent_is_zero_zero() {
    assert_eq!(counts(&[], "ghost"), (0, 0));
}

#[test]
fn counts_unions_archived_and_closed_live_children() {
    let mut p = mk("p", Status::Open, None);
    p.closed_children.push(ArchivedChild {
        id: "a1".into(),
        title: "archived".into(),
        closed_at: Utc::now(),
    });
    let c_closed = mk("cc", Status::Closed, Some("p"));
    let c_open = mk("co", Status::Open, Some("p"));
    let tasks = vec![p, c_closed, c_open];
    assert_eq!(counts(&tasks, "p"), (2, 3));
}

#[test]
fn bar_fully_empty_when_total_zero_unicode() {
    assert_eq!(bar(0, 0, Display::styled()), "░".repeat(10));
}

#[test]
fn bar_fully_empty_when_total_zero_ascii() {
    assert_eq!(bar(0, 0, Display::plain()), "-".repeat(10));
}

#[test]
fn bar_fully_filled_at_full_unicode() {
    assert_eq!(bar(10, 10, Display::styled()), "█".repeat(10));
}

#[test]
fn bar_fully_filled_at_full_ascii() {
    assert_eq!(bar(4, 4, Display::plain()), "#".repeat(10));
}

#[test]
fn bar_partial_fill_unicode() {
    // 3/10 => 3 filled, 7 empty.
    let s = bar(3, 10, Display::styled());
    let filled = s.matches('█').count();
    let empty = s.matches('░').count();
    assert_eq!(filled, 3);
    assert_eq!(empty, 7);
}

#[test]
fn bar_partial_fill_ascii() {
    let s = bar(2, 5, Display::plain());
    // 2*10/5 = 4 filled, 6 empty.
    assert_eq!(s.matches('#').count(), 4);
    assert_eq!(s.matches('-').count(), 6);
}

#[test]
fn summary_includes_bar_counts_and_percent() {
    let s = summary(3, 4, Display::plain());
    assert!(s.contains("3/4"));
    assert!(s.contains("75%"));
}

#[test]
fn summary_zero_total_yields_zero_percent() {
    let s = summary(0, 0, Display::plain());
    assert!(s.contains("0/0"));
    assert!(s.contains("0%"));
}
