//! Tests for the `bl list` compose-AND filters: tag-subset, date-window, text.

use super::*;
use crate::reads::test_support::task;
use crate::reads::Flags;

/// A task carrying `tags` and a body, created at `created`.
fn tagged(title: &str, body: &str, created: i64, tags: &[&str]) -> Task {
    let mut t = task(title, created);
    t.body = body.into();
    t.tags = tags.iter().map(ToString::to_string).collect();
    t
}

#[test]
fn an_unfiltered_list_passes_every_ball() {
    // No active filter ⇒ vacuous pass, whatever the timestamps.
    assert!(matches(&task("Anything", 42), 99, &Flags::default()));
}

#[test]
fn tag_filter_demands_every_requested_tag() {
    let t = tagged("T", "", 1, &["infra", "api"]);
    let flags = |tags: &[&str]| Flags { tags: tags.iter().map(ToString::to_string).collect(), ..Default::default() };
    assert!(matches(&t, 1, &flags(&["infra"])));
    assert!(matches(&t, 1, &flags(&["infra", "api"]))); // AND-subset
    assert!(!matches(&t, 1, &flags(&["infra", "missing"]))); // one absent ⇒ out
}

#[test]
fn date_window_matches_on_created_or_effective_updated() {
    // Created at 100, last touched at 500.
    let t = task("Spanning", 100);
    let win = |since, until| Flags { since, until, ..Default::default() };
    assert!(matches(&t, 500, &win(Some(50), Some(150)))); // created inside
    assert!(matches(&t, 500, &win(Some(400), Some(600)))); // updated inside
    assert!(!matches(&t, 500, &win(Some(200), Some(400)))); // neither inside
    assert!(matches(&t, 500, &win(Some(50), None))); // open upper bound
    assert!(matches(&t, 500, &win(None, Some(600)))); // open lower bound
}

#[test]
fn text_filter_is_a_case_insensitive_substring_of_title_or_body() {
    let t = tagged("Refactor Auth", "fixes the TIMEOUT", 1, &[]);
    let q = |needle: &str| Flags { target: Some(needle.into()), ..Default::default() };
    assert!(matches(&t, 1, &q("auth"))); // title, case-folded
    assert!(matches(&t, 1, &q("timeout"))); // body, case-folded
    assert!(!matches(&t, 1, &q("database"))); // absent in both
}

#[test]
fn the_filters_compose_with_and() {
    let t = tagged("Build the API", "with rate limiting", 100, &["api"]);
    // All three satisfied.
    let all = Flags {
        tags: vec!["api".into()],
        since: Some(50),
        until: Some(150),
        target: Some("rate".into()),
        ..Default::default()
    };
    assert!(matches(&t, 100, &all));
    // One failing predicate (wrong tag) sinks the whole AND.
    let bad_tag = Flags { tags: vec!["infra".into()], ..all };
    assert!(!matches(&t, 100, &bad_tag));
}
