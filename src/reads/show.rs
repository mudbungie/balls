//! `bl show <id>` — one ball in full. The human view is a labelled field block
//! (only present fields shown), its derived status badge, its `blockers` edges
//! annotated by what each gates (§10), the containment children that point at
//! it (§3 — `parent` is display-only), and the markdown body. `--json` is the
//! machine-contract object plus `body` and the child-id list. A missing id is an
//! error — unlike the set reads, `show` names one ball.

use std::fmt::Write;
use std::io;

use super::{json_line, on_word, status_word, task_json, Catalog, Entry, Flags, Style};
use crate::civil::iso8601;

/// Render `bl show`. Errors if `flags.target` (guaranteed present by the parser)
/// names no live ball.
pub(crate) fn render(cat: &Catalog, flags: &Flags, style: &Style) -> io::Result<String> {
    let id = flags.target.as_deref().expect("parser guarantees show has a target");
    let e = cat
        .get(id)
        .ok_or_else(|| io::Error::other(format!("no such ball: {id}")))?;
    Ok(if flags.json {
        // `--json` is the bedrock record (§9) — no derived `children`, no body,
        // identical to a `list` row. The rich view is the human projection.
        json_line(&task_json(&e.id, &e.task))
    } else {
        human(cat, e, &child_ids(cat, id), style)
    })
}

/// The ids of balls whose `parent` points at `id`, in catalog (id) order —
/// the emergent containment rollup (§10), display-only (human render).
fn child_ids<'a>(cat: &'a Catalog, id: &str) -> Vec<&'a Entry> {
    cat.entries()
        .iter()
        .filter(|c| c.task.parent.as_deref() == Some(id))
        .collect()
}

/// The human field block: badge + title, then one labelled line per present
/// field, the annotated blocker edges, the children, and the body.
fn human(cat: &Catalog, e: &Entry, children: &[&Entry], style: &Style) -> String {
    let mut out = format!("{} {}  {}\n", style.badge(cat.status(e)), e.id, e.task.title);
    field(&mut out, "status", status_word(cat.status(e)));
    field(&mut out, "created", &iso8601(e.task.created));
    field(&mut out, "updated", &iso8601(e.task.updated));
    if let Some(c) = &e.task.claimant {
        field(&mut out, "claimant", c);
    }
    if let Some(p) = e.task.priority {
        field(&mut out, "priority", &p.to_string());
    }
    if let Some(p) = &e.task.parent {
        field(&mut out, "parent", p);
    }
    if !e.task.tags.is_empty() {
        field(&mut out, "tags", &e.task.tags.join(", "));
    }
    blockers(&mut out, e);
    kids(&mut out, cat, children, style);
    if !e.task.body.is_empty() {
        out.push('\n');
        out.push_str(&e.task.body);
    }
    out
}

/// A `  label  value` line — the field block's one row shape.
fn field(out: &mut String, label: &str, value: &str) {
    let _ = writeln!(out, "  {label:<9}{value}");
}

/// The `blockers` section: each edge as `<id> (on <op>)`, annotated as a
/// dependency (`on=claim`) or gate (`on=close`) by which transition it gates.
fn blockers(out: &mut String, e: &Entry) {
    if e.task.blockers.is_empty() {
        return;
    }
    out.push_str("  blockers\n");
    for b in &e.task.blockers {
        let _ = writeln!(out, "    {} (on {})", b.id, on_word(b.on));
    }
}

/// The containment children, each as its own badge line under a `children`
/// header — empty containment prints nothing.
fn kids(out: &mut String, cat: &Catalog, children: &[&Entry], style: &Style) {
    if children.is_empty() {
        return;
    }
    out.push_str("  children\n");
    for c in children {
        let _ = writeln!(out, "    {} {}  {}", style.badge(cat.status(c)), c.id, c.task.title);
    }
}

#[cfg(test)]
#[path = "show_tests.rs"]
mod tests;
