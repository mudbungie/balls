//! Coverage tests for the show-text helpers.

use super::*;
use crate::display::Display;
use chrono::{TimeZone, Utc};

fn make_ctx<'a>(now: chrono::DateTime<Utc>, verbose: bool) -> super::super::render_show::Ctx<'a> {
    super::super::render_show::Ctx {
        d: Display::plain(),
        me: "me",
        columns: 80,
        verbose,
        now,
    }
}

#[test]
fn relative_time_buckets() {
    let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
    assert_eq!(relative_time(now, now), "just now");
    let m = now - chrono::Duration::seconds(120);
    assert_eq!(relative_time(m, now), "2m ago");
    let h = now - chrono::Duration::hours(3);
    assert_eq!(relative_time(h, now), "3h ago");
    let d = now - chrono::Duration::days(5);
    assert_eq!(relative_time(d, now), "5d ago");
}

#[test]
fn relative_time_handles_negative_drift() {
    // ts in the future (clock skew) — abs value is taken.
    let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
    let future = now + chrono::Duration::minutes(5);
    assert_eq!(relative_time(future, now), "5m ago");
}

#[test]
fn format_time_appends_iso_when_verbose() {
    let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
    let ts = now - chrono::Duration::hours(1);
    let ctx = make_ctx(now, true);
    let s = format_time(ts, &ctx);
    assert!(s.starts_with("1h ago"));
    assert!(s.contains('('));
    assert!(s.ends_with(')'));
}

#[test]
fn format_time_relative_only_by_default() {
    let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
    let ctx = make_ctx(now, false);
    let s = format_time(now - chrono::Duration::minutes(5), &ctx);
    assert_eq!(s, "5m ago");
}

#[test]
fn wrap_short_paragraph_passes_through() {
    let v = wrap("hello world", 80);
    assert_eq!(v, vec!["hello world".to_string()]);
}

#[test]
fn wrap_long_paragraph_breaks_on_word_boundary() {
    let v = wrap("aaa bbb ccc ddd eee fff", 10);
    assert!(v.iter().all(|line| line.len() <= 10));
    assert!(v.len() > 1);
}

#[test]
fn wrap_preserves_paragraph_breaks() {
    let v = wrap("first\n\nsecond", 80);
    assert_eq!(v, vec!["first".to_string(), String::new(), "second".to_string()]);
}

#[test]
fn wrap_handles_empty_paragraph() {
    let v = wrap("", 80);
    // Trailing empty paragraphs get trimmed; result is empty.
    assert!(v.is_empty());
}

#[test]
fn wrap_word_longer_than_width_appears_on_its_own_line() {
    let v = wrap("ab supercalifragilisticexpialidocious cd", 5);
    assert!(v.iter().any(|line| line == "supercalifragilisticexpialidocious"));
}
