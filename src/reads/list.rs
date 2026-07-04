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
use std::io;
use std::path::Path;

use serde_json::Value;

use super::history::Dead;
use super::{claim_age, filter, json_line, task_json, Catalog, Entry, Flags, Style};
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
/// `store`/`now` feed the derived claim-age on live claimed rows (human render
/// only, bl-46ef); the `--json` and unclaimed/dead paths never walk the store.
pub(crate) fn render_list(cat: &Catalog, dead: &[Dead], flags: &Flags, style: &Style, store: &Path, now: i64) -> io::Result<String> {
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
    render(cat, &rows, flags, style, store, now)
}

/// The §10 display order of a row: `(absent-priority, priority, created, id)`.
/// `priority.is_none()` sorts `true` last, so a no-priority ball follows every
/// prioritised one; ties break by `created` then id. Uniform over live and dead.
fn order_key<'a>(r: &'a Row) -> (bool, i64, i64, &'a str) {
    let t = r.task();
    (t.priority.is_none(), t.priority.unwrap_or(0), t.created, r.id())
}

/// Render `rows` either as the `--json` array or as badge lines. `--json`
/// returns before any store walk — it stays the bedrock stored-frontmatter
/// mirror (§3), so the derived age is a human-render column alone (bl-46ef).
fn render(cat: &Catalog, rows: &[Row], flags: &Flags, style: &Style, store: &Path, now: i64) -> io::Result<String> {
    if flags.json {
        let arr: Vec<Value> = rows.iter().map(|r| task_json(r.id(), r.task())).collect();
        return Ok(json_line(&Value::Array(arr)));
    }
    let mut out = String::new();
    for r in rows {
        let age = age_hint(r, flags, store, now)?;
        out.push_str(&line(&badge(cat, r, style), r.id(), r.task(), &age));
    }
    Ok(out)
}

/// The ` (<age>)` suffix a LIVE claimed row hangs on its `@claimant` — the
/// claim's age in one coarse unit (§9 derived, human render only). Everything
/// else yields `""` AND pays no walk: a dead row renders retirement not
/// claim-age, an unclaimed row has no claimant to hang it on, and a `--legacy`
/// read's history lives on the legacy ref, not this store (§16). A claimed row
/// with no claim commit behind it (hand-set claimant) also renders bare.
fn age_hint(r: &Row, flags: &Flags, store: &Path, now: i64) -> io::Result<String> {
    let Row::Live(e) = r else { return Ok(String::new()) };
    if flags.legacy.is_some() || e.task.claimant.is_none() {
        return Ok(String::new());
    }
    let hint = claim_age::claimed_at(store, &e.id)?
        .map_or(String::new(), |t| format!(" ({})", claim_age::humanize(now - t)));
    Ok(hint)
}

/// The badge for a row: the live status ladder, or the dead `closed` word/glyph.
fn badge(cat: &Catalog, r: &Row, style: &Style) -> String {
    match r {
        Row::Live(e) => style.badge(cat.status(e)),
        Row::Dead(_) => style.retired_badge(),
    }
}

/// One human row: `<badge> <id>  <title>` plus a `pN` priority hint and an
/// `@claimant` occupancy hint when present. `age` is the derived claim-age
/// suffix (` (3h)`) for a live claimed row, `""` otherwise — it rides the
/// claimant, not a free-floating column (bl-46ef).
fn line(badge: &str, id: &str, task: &Task, age: &str) -> String {
    let mut row = format!("{badge} {id}  {}", task.title);
    if let Some(p) = task.priority {
        let _ = write!(row, "  p{p}");
    }
    if let Some(c) = &task.claimant {
        let _ = write!(row, "  @{c}{age}");
    }
    row.push('\n');
    row
}

impl Catalog {
    /// The parsed balls, id-sorted at load — the row source for `list` and
    /// `show`'s children scan.
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
