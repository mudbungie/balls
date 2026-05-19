//! Rich rendering for `bl show`.
//!
//! Lays out a header line (priority + status + id + title + claimed
//! badge), a compact metadata row (type, relative timestamps, tags),
//! a relations block (deps with inline statuses, gates, parent,
//! children, delivered/branch/remote meta), a wrapped description,
//! and an oldest-first notes log. Relative timestamps use a `now`
//! passed in so the whole module is pure and testable.

use crate::delivery::Delivery;
use crate::display::Display;
use crate::progress;
use crate::render_show_relations::write_relations;
use crate::render_show_text::{format_time, wrap};
use crate::sanitize;
use crate::task::Task;
use chrono::{DateTime, Utc};
use std::fmt::Write as _;
use std::path::Path;

pub struct Ctx<'a> {
    pub d: Display,
    pub me: &'a str,
    pub columns: usize,
    pub verbose: bool,
    pub now: DateTime<Utc>,
}

pub fn render(
    task: &Task,
    all: &[Task],
    delivery: &Delivery,
    repo_root: &Path,
    ctx: &Ctx<'_>,
) -> String {
    let mut out = String::new();
    write_header(&mut out, task, ctx);
    write_meta_row(&mut out, task, ctx);
    write_tags(&mut out, task);
    write_progress(&mut out, task, all, ctx);
    write_relations(&mut out, task, all, delivery, repo_root, ctx);
    write_description(&mut out, task, ctx.columns);
    write_notes(&mut out, task, ctx);
    out
}

fn write_progress(out: &mut String, t: &Task, all: &[Task], ctx: &Ctx<'_>) {
    if !t.task_type.is_epic() {
        return;
    }
    let (closed, total) = progress::counts(all, &t.id);
    let _ = writeln!(out, "  progress: {}", progress::summary(closed, total, ctx.d));
}

fn write_header(out: &mut String, t: &Task, ctx: &Ctx<'_>) {
    let claimed = match &t.claimed_by {
        Some(c) if !c.is_empty() => format!(
            "  {} claimed by {c}",
            if ctx.d.use_unicode() { "★" } else { "*" },
        ),
        _ => String::new(),
    };
    let _ = writeln!(
        out,
        "{} {} {} {}  {}{claimed}",
        ctx.d.prio_dot(t.priority),
        ctx.d.status_glyph(&t.status),
        ctx.d.status_word(&t.status),
        t.id,
        sanitize::inline(&t.title),
    );
}

fn write_meta_row(out: &mut String, t: &Task, ctx: &Ctx<'_>) {
    let created = format_time(t.created_at, ctx);
    let updated = format_time(t.updated_at, ctx);
    let _ = writeln!(
        out,
        "  type: {type_name}      created: {created}       updated: {updated}",
        type_name = t.task_type.as_str(),
    );
}

fn write_tags(out: &mut String, t: &Task) {
    if t.tags.is_empty() {
        return;
    }
    let _ = writeln!(out, "  tags: {}", sanitize::inline(&t.tags.join(", ")));
}

fn write_description(out: &mut String, t: &Task, columns: usize) {
    if t.description.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str("  description\n");
    for line in wrap(&sanitize::block(&t.description), columns.saturating_sub(4).max(1)) {
        let _ = writeln!(out, "    {line}");
    }
}

fn write_notes(out: &mut String, t: &Task, ctx: &Ctx<'_>) {
    if t.notes.is_empty() {
        return;
    }
    out.push('\n');
    let _ = writeln!(out, "  notes ({})", t.notes.len());
    for n in &t.notes {
        let when = format_time(n.ts, ctx);
        let (au, tx) = (sanitize::inline(&n.author), sanitize::inline(&n.text));
        let _ = writeln!(out, "    {when}  {au}  — {tx}");
    }
}

#[cfg(test)]
#[path = "render_show_tests.rs"]
mod tests;
