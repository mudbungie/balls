//! `bl list` — the set-of-balls read, one line per ball.
//!
//! `list` is the SINGLE listing verb (§9 — the old `bl ready` folded in as the
//! `--status ready` filter). It defaults to the live/open set, optionally
//! narrowed to one §3 status rung by `flags.status`, and reaches the dead set
//! through `--status closed`/`--all` (history-reconstructed, §9). Every row — live or
//! dead — is then put through the compose-AND [`filter`]s and ORDERED the §10
//! way: `priority` ascending (absent LAST), then `created` ascending, with id as
//! a stable final tiebreak. Ordering is uniform — ready's order was never
//! special — and display-only: it never enters the `ready()` predicate (§3/§10).

use std::fmt::Write;

use serde_json::Value;

use super::history::Dead;
use super::{filter, json_line, task_json, Catalog, Entry, Flags, Style};
use crate::task::Task;

/// One listed row — a live catalog [`Entry`] or a history-reconstructed [`Dead`]
/// ball. Both expose the same frontmatter for ordering, filtering, and the
/// bedrock `--json`; they differ only in their badge (the live status ladder vs
/// the retirement) and the effective date the filters read.
enum Row<'a> {
    Live(&'a Entry),
    Dead(&'a Dead),
}

impl Row<'_> {
    /// The ball id — the filename identity (§3), shared by both kinds.
    fn id(&self) -> &str {
        match self {
            Row::Live(e) => &e.id,
            Row::Dead(d) => &d.id,
        }
    }

    /// The stored frontmatter+body — the ordering/filter/bedrock source.
    fn task(&self) -> &Task {
        match self {
            Row::Live(e) => &e.task,
            Row::Dead(d) => &d.task,
        }
    }
}

/// `bl list` — the live set (or one `--status` rung), plus the reconstructed
/// `dead` set when the reach calls for it, every row filtered and §10-ordered.
/// `--json` emits the array of bedrock objects; otherwise one badge line each.
pub(crate) fn render_list(cat: &Catalog, dead: &[Dead], flags: &Flags, style: &Style) -> String {
    let mut rows: Vec<Row> = Vec::new();
    if flags.reach.live() {
        // The status filter is the LIVE ladder alone (§9); dead balls left no rung.
        rows.extend(
            cat.entries()
                .iter()
                .filter(|e| flags.status.is_none_or(|want| cat.status(e) == want))
                .filter(|e| filter::matches(&e.task, e.task.updated, flags))
                .map(Row::Live),
        );
    }
    if flags.reach.dead() {
        rows.extend(
            dead.iter().filter(|d| filter::matches(&d.task, d.retired_at, flags)).map(Row::Dead),
        );
    }
    rows.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
    render(cat, &rows, flags, style)
}

/// The §10 display order of a row: `(absent-priority, priority, created, id)`.
/// `priority.is_none()` sorts `true` last, so a no-priority ball follows every
/// prioritised one; ties break by `created` then id. Uniform over live and dead.
fn order_key<'a>(r: &'a Row) -> (bool, i64, i64, &'a str) {
    let t = r.task();
    (t.priority.is_none(), t.priority.unwrap_or(0), t.created, r.id())
}

/// Render `rows` either as the `--json` array or as badge lines.
fn render(cat: &Catalog, rows: &[Row], flags: &Flags, style: &Style) -> String {
    if flags.json {
        let arr: Vec<Value> = rows.iter().map(|r| task_json(r.id(), r.task())).collect();
        return json_line(&Value::Array(arr));
    }
    let mut out = String::new();
    for r in rows {
        out.push_str(&line(&badge(cat, r, style), r.id(), r.task()));
    }
    out
}

/// The badge for a row: the live status ladder, or the dead `closed` word/glyph.
fn badge(cat: &Catalog, r: &Row, style: &Style) -> String {
    match r {
        Row::Live(e) => style.badge(cat.status(e)),
        Row::Dead(_) => style.retired_badge(),
    }
}

/// One human row: `<badge> <id>  <title>` plus a `pN` priority hint and an
/// `@claimant` occupancy hint when present.
fn line(badge: &str, id: &str, task: &Task) -> String {
    let mut row = format!("{badge} {id}  {}", task.title);
    if let Some(p) = task.priority {
        let _ = write!(row, "  p{p}");
    }
    if let Some(c) = &task.claimant {
        let _ = write!(row, "  @{c}");
    }
    row.push('\n');
    row
}

impl Catalog {
    /// The parsed balls, id-sorted at load — the shared row source for `list`,
    /// `ready`, and `dep-tree`.
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
