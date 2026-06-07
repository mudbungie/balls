//! `bl list` and `bl ready` — the set-of-balls reads, one line per ball.
//!
//! `list` shows every live ball in id order with its §3 status badge. `ready`
//! shows only the [`Status::Ready`] rung and ORDERS it the §10 way: `priority`
//! ascending (absent LAST), then `created` ascending, with id as a stable final
//! tiebreak. The ordering is display-only — it never enters the `ready()`
//! predicate (§3/§10).

use std::fmt::Write;

use serde_json::Value;

use super::{json_line, task_json, Catalog, Entry, Flags, Style};
use crate::task::Status;

/// `bl list` — every live ball, id order. `--json` emits the array of
/// machine-contract objects; otherwise one badge line per ball.
pub(crate) fn render_list(cat: &Catalog, flags: &Flags, style: &Style) -> String {
    let rows: Vec<&Entry> = cat.entries().iter().collect();
    render(cat, &rows, flags, style)
}

/// `bl ready` — the ready set, §10-ordered. Same rendering as `list` over the
/// filtered, sorted rows.
pub(crate) fn render_ready(cat: &Catalog, flags: &Flags, style: &Style) -> String {
    let mut rows: Vec<&Entry> = cat
        .entries()
        .iter()
        .filter(|e| cat.status(e) == Status::Ready)
        .collect();
    rows.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
    render(cat, &rows, flags, style)
}

/// The §10 display order of a ready ball: `(absent-priority, priority, created,
/// id)`. `priority.is_none()` sorts `true` last, so a no-priority ball follows
/// every prioritised one; ties break by `created` then id.
fn order_key(e: &Entry) -> (bool, i64, i64, &str) {
    (e.task.priority.is_none(), e.task.priority.unwrap_or(0), e.task.created, &e.id)
}

/// Render `rows` either as the `--json` array or as badge lines.
fn render(cat: &Catalog, rows: &[&Entry], flags: &Flags, style: &Style) -> String {
    if flags.json {
        let arr: Vec<Value> = rows.iter().map(|e| task_json(&e.id, &e.task)).collect();
        return json_line(&Value::Array(arr));
    }
    let mut out = String::new();
    for e in rows {
        out.push_str(&line(cat, e, style));
    }
    out
}

/// One human row: `<badge> <id>  <title>` plus a `pN` priority hint and an
/// `@claimant` occupancy hint when present.
fn line(cat: &Catalog, e: &Entry, style: &Style) -> String {
    let mut row = format!("{} {}  {}", style.badge(cat.status(e)), e.id, e.task.title);
    if let Some(p) = e.task.priority {
        let _ = write!(row, "  p{p}");
    }
    if let Some(c) = &e.task.claimant {
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
