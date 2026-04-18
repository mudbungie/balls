//! Single-status (always open) ready-queue rendering.
//!
//! Shape matches `bl list`'s flat mode so the eye doesn't retrain
//! between commands: priority dot + status glyph + status word + id +
//! claimed badge + title + tags. The new piece here is the parent
//! hint — `↑ bl-xxxx (title)` rendered dim — appended when a task
//! has a parent, so an agent picking work from `bl ready` doesn't
//! lose the surrounding epic.

use crate::display::Display;
use crate::task::{Status, Task};
use owo_colors::OwoColorize;

pub fn render(ready: &[&Task], all: &[Task], d: Display, me: &str) -> String {
    let mut out = String::new();
    for t in ready {
        out.push_str(&format_row(t, all, d, me));
        out.push('\n');
    }
    out
}

fn format_row(t: &Task, all: &[Task], d: Display, me: &str) -> String {
    let claimed = d.claimed_badge(t, me);
    let badge_segment = if claimed.is_empty() {
        String::new()
    } else {
        format!("{claimed} ")
    };
    let tags = if t.tags.is_empty() {
        String::new()
    } else {
        format!("  {}", t.tags.join(", "))
    };
    let mut row = format!(
        "{} {} {:<12} {} {}{}{}",
        d.prio_dot(t.priority),
        d.status_glyph(&Status::Open),
        d.status_word(&Status::Open),
        t.id,
        badge_segment,
        t.title,
        tags,
    );
    if let Some(hint) = parent_hint(t, all, d) {
        row.push_str("  ");
        row.push_str(&hint);
    }
    row
}

fn parent_hint(t: &Task, all: &[Task], d: Display) -> Option<String> {
    let pid = t.parent.as_ref()?;
    let parent = all.iter().find(|p| &p.id == pid)?;
    let arrow = if d.use_unicode() { "↑" } else { "^" };
    let raw = format!("{arrow} {} ({})", parent.id, parent.title);
    Some(if d.use_color() {
        raw.dimmed().to_string()
    } else {
        raw
    })
}

#[cfg(test)]
#[path = "render_ready_tests.rs"]
mod tests;
