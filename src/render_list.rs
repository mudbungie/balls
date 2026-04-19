//! Grouped/nested/glyphed rendering for `bl list`.
//!
//! The single-line plain format is replaced with a status-grouped
//! layout: headers split the output by status, tasks nest under their
//! in-group parents, and every row carries priority dot + status
//! glyph + status word + id + badges + title + tags.
//!
//! Width-aware truncation: if the row won't fit in `columns`, tags
//! are dropped first, then the title is trimmed with an ellipsis.
//! ANSI escape sequences only appear inside the *prefix* (priority
//! dot, status word, badges) — the title is literal — so truncation
//! can chop the title without corrupting the colored prefix.

use crate::display::Display;
use crate::progress;
use crate::task::{Status, Task, TaskType};
use std::collections::HashSet;
use std::fmt::Write;

/// Eye-scan order: what's moving now first, what's ready to pick next
/// second, then parked categories. Closed is filtered out upstream.
const GROUP_ORDER: &[Status] = &[
    Status::InProgress,
    Status::Review,
    Status::Open,
    Status::Blocked,
    Status::Deferred,
];

/// Bundles every read-only knob the renderer needs so individual
/// helpers don't drag a 6-arg signature around.
pub struct Ctx<'a> {
    pub d: Display,
    pub me: &'a str,
    pub columns: usize,
    pub all: &'a [Task],
}

/// Render a filtered task set. `flat` true means `--status` was
/// supplied — no grouping, but the status column stays for visual
/// consistency with the grouped view.
pub fn render(tasks: &[Task], flat: bool, ctx: &Ctx<'_>) -> String {
    if flat {
        return render_flat(tasks, ctx);
    }
    let mut out = String::new();
    let mut first = true;
    for status in GROUP_ORDER {
        let group: Vec<&Task> = tasks.iter().filter(|t| &t.status == status).collect();
        if group.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        let _ = writeln!(
            out,
            "{} {}",
            ctx.d.status_glyph(status),
            ctx.d.status_word(status),
        );
        let ids_in_group: HashSet<&str> = group.iter().map(|t| t.id.as_str()).collect();
        let roots: Vec<&Task> = group
            .iter()
            .filter(|t| {
                t.parent
                    .as_deref()
                    .is_none_or(|p| !ids_in_group.contains(p))
            })
            .copied()
            .collect();
        for r in sorted(roots) {
            emit_nested(&mut out, r, &group, 0, ctx);
        }
    }
    out
}

fn render_flat(tasks: &[Task], ctx: &Ctx<'_>) -> String {
    let mut out = String::new();
    for t in sorted(tasks.iter().collect()) {
        out.push_str(&format_row(t, 0, ctx));
        out.push('\n');
    }
    out
}

fn emit_nested(out: &mut String, t: &Task, group: &[&Task], depth: usize, ctx: &Ctx<'_>) {
    out.push_str(&format_row(t, depth, ctx));
    out.push('\n');
    let kids: Vec<&Task> = group
        .iter()
        .filter(|c| c.parent.as_deref() == Some(t.id.as_str()))
        .copied()
        .collect();
    for k in sorted(kids) {
        emit_nested(out, k, group, depth + 1, ctx);
    }
}

fn sorted(mut v: Vec<&Task>) -> Vec<&Task> {
    v.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
    v
}

fn format_row(t: &Task, depth: usize, ctx: &Ctx<'_>) -> String {
    let indent = "  ".repeat(depth);
    let claimed = ctx.d.claimed_badge(t, ctx.me);
    let deps = ctx.d.deps_badge(t, ctx.all);
    let gates = ctx.d.gates_badge(t, ctx.all);
    let mut badges = String::new();
    for b in [claimed, deps, gates] {
        if !b.is_empty() {
            badges.push_str(b);
            badges.push(' ');
        }
    }
    // Prefix carries every styled byte; title and tags are literal.
    let prefix_styled = format!(
        "{} {} {:<12} {}{} {}",
        ctx.d.prio_dot(t.priority),
        ctx.d.status_glyph(&t.status),
        ctx.d.status_word(&t.status),
        indent,
        t.id,
        badges,
    );
    let tags = if t.tags.is_empty() {
        String::new()
    } else {
        t.tags.join(", ")
    };
    let title = epic_title(t, ctx);
    fit(&prefix_styled, &title, &tags, ctx.columns)
}

/// Titles of `type=epic` tasks carry an `[epic]` marker and a 10-cell
/// progress bar so the epic row scans as a container at a glance.
/// Other types render bare titles.
fn epic_title(t: &Task, ctx: &Ctx<'_>) -> String {
    if !matches!(t.task_type, TaskType::Epic) {
        return t.title.clone();
    }
    let (closed, total) = progress::counts(ctx.all, &t.id);
    format!("{}  [epic]  {}", t.title, progress::summary(closed, total, ctx.d))
}

fn fit(prefix_styled: &str, title: &str, tags: &str, columns: usize) -> String {
    // Measure prefix width by stripping SGR escapes. `Display` only
    // emits `\x1b[...m`, so this stays accurate without dragging in
    // a full ANSI parser.
    let pv = strip_ansi_len(prefix_styled);
    let title_w = title.chars().count();
    let tags_w = tags.chars().count();
    let with_tags = pv + title_w + if tags.is_empty() { 0 } else { 2 + tags_w };
    if with_tags <= columns {
        if tags.is_empty() {
            return format!("{prefix_styled}{title}");
        }
        return format!("{prefix_styled}{title}  {tags}");
    }
    if pv + title_w <= columns {
        return format!("{prefix_styled}{title}");
    }
    let keep = columns.saturating_sub(pv).saturating_sub(1);
    let truncated: String = title.chars().take(keep).collect();
    format!("{prefix_styled}{truncated}…")
}

/// Character count ignoring ANSI SGR (`\x1b[...m`) escapes.
fn strip_ansi_len(s: &str) -> usize {
    let mut count = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        count += 1;
    }
    count
}

#[cfg(test)]
#[path = "render_list_tests.rs"]
mod tests;
