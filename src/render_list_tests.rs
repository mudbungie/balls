//! Coverage-targeted tests for the grouped list renderer.

use super::*;
use crate::display::Display;
use crate::task::{NewTaskOpts, Status, Task};

fn mk(id: &str, title: &str, status: Status, priority: u8) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: title.into(),
            priority,
            ..Default::default()
        },
        id.into(),
    );
    t.status = status;
    t
}

fn with_parent(mut t: Task, parent: &str) -> Task {
    t.parent = Some(parent.into());
    t
}

fn with_tags(mut t: Task, tags: &[&str]) -> Task {
    t.tags = tags.iter().map(|s| String::from(*s)).collect();
    t
}

fn ctx<'a>(me: &'a str, columns: usize, all: &'a [Task]) -> Ctx<'a> {
    Ctx {
        d: Display::plain(),
        me,
        columns,
        all,
    }
}

#[test]
fn flat_mode_prints_one_line_per_task_no_headers() {
    let tasks = vec![
        mk("bl-1", "alpha", Status::Open, 2),
        mk("bl-2", "beta", Status::Open, 1),
    ];
    let out = render(&tasks, true, &ctx("nobody", 120, &tasks));
    assert!(!out.starts_with("[ ] open"));
    let b1 = out.find("bl-1").unwrap();
    let b2 = out.find("bl-2").unwrap();
    assert!(b2 < b1);
}

#[test]
fn grouped_mode_emits_header_per_status() {
    let tasks = vec![
        mk("bl-1", "in_prog_t", Status::InProgress, 3),
        mk("bl-2", "open_t", Status::Open, 3),
    ];
    let out = render(&tasks, false, &ctx("nobody", 120, &tasks));
    let ip = out.find("[>] in_progress").unwrap();
    let op = out.find("[ ] open").unwrap();
    assert!(ip < op);
}

#[test]
fn grouped_empty_group_skipped() {
    let tasks = vec![mk("bl-1", "only_open", Status::Open, 3)];
    let out = render(&tasks, false, &ctx("nobody", 120, &tasks));
    assert!(!out.contains("in_progress"));
    assert!(out.contains("only_open"));
}

#[test]
fn grouped_blank_line_between_groups() {
    let tasks = vec![
        mk("bl-1", "ip", Status::InProgress, 3),
        mk("bl-2", "op", Status::Open, 3),
    ];
    let out = render(&tasks, false, &ctx("nobody", 120, &tasks));
    assert!(out.contains("\n\n"));
}

#[test]
fn children_nest_under_parent_in_same_group() {
    let parent = mk("bl-p", "parent", Status::Open, 3);
    let child = with_parent(mk("bl-c", "child", Status::Open, 3), "bl-p");
    let tasks = vec![parent, child];
    let out = render(&tasks, false, &ctx("nobody", 120, &tasks));
    let p = out.find("parent").unwrap();
    let c = out.find("child").unwrap();
    assert!(p < c);
    assert!(out.contains("  bl-c"));
}

#[test]
fn child_in_different_group_renders_as_root() {
    let parent = mk("bl-p", "parent", Status::InProgress, 3);
    let child = with_parent(mk("bl-c", "child", Status::Open, 3), "bl-p");
    let tasks = vec![parent, child];
    let out = render(&tasks, false, &ctx("nobody", 120, &tasks));
    assert!(out.contains("[ ] open"));
    assert!(out.contains("child"));
}

#[test]
fn tags_render_when_row_fits() {
    let t = with_tags(mk("bl-1", "tagged", Status::Open, 3), &["api", "auth"]);
    let tasks = std::slice::from_ref(&t);
    let out = render(tasks, true, &ctx("nobody", 120, tasks));
    assert!(out.contains("api, auth"));
}

#[test]
fn tags_dropped_when_row_overflow() {
    let t = with_tags(
        mk("bl-1", "a_reasonably_long_title_of_content", Status::Open, 3),
        &["first", "second", "third"],
    );
    let tasks = std::slice::from_ref(&t);
    let out = render(tasks, true, &ctx("nobody", 40, tasks));
    assert!(!out.contains("first, second"));
}

#[test]
fn title_truncated_when_prefix_plus_title_exceeds_columns() {
    let t = mk(
        "bl-abcd",
        "a_very_very_very_very_very_long_title_indeed_here",
        Status::Open,
        3,
    );
    let tasks = std::slice::from_ref(&t);
    let out = render(tasks, true, &ctx("nobody", 30, tasks));
    assert!(out.contains('…'));
}

#[test]
fn strip_ansi_handles_escape_and_non_escape_chars() {
    let s = "\x1b[31mhello\x1b[0m world";
    assert_eq!(strip_ansi_len(s), "hello world".chars().count());
    assert_eq!(strip_ansi_len("plain"), 5);
}

#[test]
fn fit_no_tags_fits_returns_prefix_plus_title() {
    let out = fit("P ", "title", "", 80);
    assert_eq!(out, "P title");
}

#[test]
fn fit_with_tags_pads_and_appends() {
    let out = fit("P ", "t", "tag", 80);
    assert!(out.starts_with("P t"));
    assert!(out.ends_with("tag"));
}

#[test]
fn fit_tags_dropped_when_overflow_but_title_fits() {
    let out = fit("P ", "title", "many, tags, here", 10);
    assert_eq!(out, "P title");
}

#[test]
fn fit_truncates_title_when_everything_overflows() {
    let out = fit("P ", "longertitle", "", 6);
    assert_eq!(out, "P lon…");
}

#[test]
fn claimed_badge_and_deps_badge_appear_in_styled_prefix() {
    let mut parent = mk("bl-p", "p", Status::Open, 3);
    parent.claimed_by = Some("me".into());
    let blocker = mk("bl-b", "blocker", Status::Open, 3);
    let mut child = mk("bl-c", "c", Status::Open, 3);
    child.depends_on = vec!["bl-b".into()];
    let tasks = vec![parent, blocker, child];
    let out = render(&tasks, true, &ctx("me", 200, &tasks));
    assert!(out.contains('*')); // claimed badge ascii
    assert!(out.contains('D')); // deps badge ascii
}

#[test]
fn grouped_covers_all_five_statuses() {
    let tasks = vec![
        mk("bl-1", "ip", Status::InProgress, 3),
        mk("bl-2", "rv", Status::Review, 3),
        mk("bl-3", "op", Status::Open, 3),
        mk("bl-4", "bl", Status::Blocked, 3),
        mk("bl-5", "df", Status::Deferred, 3),
    ];
    let out = render(&tasks, false, &ctx("nobody", 200, &tasks));
    for word in ["in_progress", "review", "open", "blocked", "deferred"] {
        assert!(out.contains(word), "missing {word} in output");
    }
}

#[test]
fn epic_rows_carry_progress_bar_and_epic_marker() {
    let mut parent = mk("bl-e", "epic row", Status::Open, 3);
    parent.task_type = crate::task::TaskType::Epic;
    let mut child = mk("bl-c", "child", Status::Closed, 3);
    child.parent = Some("bl-e".into());
    let tasks = vec![parent, child];
    let out = render(&tasks, true, &ctx("nobody", 200, &tasks));
    assert!(out.contains("[epic]"));
    // 1/1 closed => bar fully filled in ascii.
    assert!(out.contains("##########"));
    assert!(out.contains("1/1"));
    assert!(out.contains("100%"));
}

#[test]
fn sorted_by_priority_then_created_at() {
    let mut a = mk("bl-1", "a", Status::Open, 2);
    let b = mk("bl-2", "b", Status::Open, 2);
    a.created_at = b.created_at + chrono::Duration::seconds(1);
    let tasks = vec![a, b];
    let out = render(&tasks, true, &ctx("nobody", 120, &tasks));
    let ai = out.find("bl-1").unwrap();
    let bi = out.find("bl-2").unwrap();
    assert!(bi < ai);
}
