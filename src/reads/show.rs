//! `bl show <id>` — one ball in full, resolved by RECENCY (§9, § id generation):
//! the live `tasks/<id>.md` first, else the most recent incarnation
//! reconstructed from history. The human view is a labelled field block (only
//! present fields shown), its derived status badge, its `blockers` edges
//! annotated by what each gates (§10), the containment children that point at it
//! (§3 — `parent` is display-only), and the markdown body. `--json` is the
//! bedrock record. A dead ball renders the same block with its retirement
//! and deletion date in place of the live status; an id that
//! resolves to neither a live nor a dead ball is an error.

use std::fmt::Write;
use std::io;
use std::path::Path;

use super::history::{resolve_dead, Dead};
use super::{json_line, on_word, status_word, task_json, Catalog, Entry, Flags, Style};
use crate::civil::iso8601;
use crate::task::Task;

/// Resolve and render `bl show`. Live `tasks/<id>.md` wins; on a miss the recency
/// walk reconstructs the most recent dead incarnation from `balls/tasks` history
/// (§9); an id matching neither is an error. `flags.target` is parser-guaranteed.
/// `folded` is the §6 read-dispatch contribution — wired plugins' captured stdout
/// (the delivery worktree line, §11) — inserted verbatim into the human field
/// block; empty under `--json` (which never dispatches) or when nothing printed.
pub(crate) fn dispatch(store: &Path, cat: &Catalog, flags: &Flags, style: &Style, folded: &str) -> io::Result<String> {
    let id = flags.target.as_deref().expect("parser guarantees show has a target");
    // A corrupt ball is PRESENT, not dead (bl-528c): surface its parse error
    // rather than fall through to history and resurrect a stale incarnation.
    if let Some(err) = cat.corruption(id) {
        return Err(io::Error::other(format!("tasks/{id}.md: {err}")));
    }
    match cat.get(id) {
        Some(e) => Ok(render_live(cat, e, flags, style, folded)),
        // A `--legacy` miss never falls through to the GREENFIELD store's
        // history — the legacy set is the whole world the flag names (§16).
        None if flags.legacy.is_some() => Err(io::Error::other(format!("no such legacy ball: {id}"))),
        None => match resolve_dead(store, id)? {
            Some(dead) => Ok(render_dead(&dead, flags, style, folded)),
            None => Err(io::Error::other(format!("no such ball: {id}"))),
        },
    }
}

/// Render a live ball: the bedrock record under `--json`, else the human field
/// block (badge, fields, blockers, children + the folded plugin lines, body).
fn render_live(cat: &Catalog, e: &Entry, flags: &Flags, style: &Style, folded: &str) -> String {
    if flags.json {
        // `--json` is the bedrock record (§9) — the whole stored file (body
        // included, no derived `children`), identical to a `list` row and
        // re-ingestable by `bl import`. The rich view is the human projection.
        return json_line(&task_json(&e.id, &e.task));
    }
    let badge = style.badge(cat.status(e));
    let mut out = header(&badge, &e.id, &e.task);
    field(&mut out, "status", status_word(cat.status(e)));
    body_block(&mut out, &e.task, |out| {
        kids(out, cat, &child_ids(cat, &e.id), style);
        out.push_str(folded);
    });
    out
}

/// Render a dead (history-served) ball: the same bedrock `--json` record (its
/// reconstructed frontmatter round-trips), else the human block with the
/// retirement badge and an extra `retired` date line in place of the live status.
fn render_dead(d: &Dead, flags: &Flags, style: &Style, folded: &str) -> String {
    if flags.json {
        return json_line(&task_json(&d.id, &d.task));
    }
    let badge = style.retired_badge();
    let mut out = header(&badge, &d.id, &d.task);
    field(&mut out, "status", "closed");
    field(&mut out, "retired", &iso8601(d.retired_at));
    // Dead balls render no children rollup; a read-dispatch line still folds
    // (in practice none — a retired ball's worktree is torn down, §11).
    body_block(&mut out, &d.task, |out| out.push_str(folded));
    out
}

/// The shared `<badge> <id>  <title>` heading both kinds open with.
fn header(badge: &str, id: &str, task: &Task) -> String {
    format!("{badge} {id}  {}\n", task.title)
}

/// The fields, blockers, kind-specific `extra` section, and body common to both
/// renders. `extra` injects the live children rollup (dead balls pass a no-op).
fn body_block(out: &mut String, task: &Task, extra: impl FnOnce(&mut String)) {
    field(out, "created", &iso8601(task.created));
    field(out, "updated", &iso8601(task.updated));
    if let Some(c) = &task.claimant {
        field(out, "claimant", c);
    }
    if let Some(p) = task.priority {
        field(out, "priority", &p.to_string());
    }
    if let Some(p) = &task.parent {
        field(out, "parent", p);
    }
    if !task.tags.is_empty() {
        field(out, "tags", &task.tags.join(", "));
    }
    blockers(out, task);
    extra(out);
    if !task.body.is_empty() {
        out.push('\n');
        out.push_str(&task.body);
    }
}

/// The ids of balls whose `parent` points at `id`, in catalog (id) order —
/// the emergent containment rollup (§10), display-only (human render).
fn child_ids<'a>(cat: &'a Catalog, id: &str) -> Vec<&'a Entry> {
    cat.entries()
        .iter()
        .filter(|c| c.task.parent.as_deref() == Some(id))
        .collect()
}

/// A `  label  value` line — the field block's one row shape.
fn field(out: &mut String, label: &str, value: &str) {
    let _ = writeln!(out, "  {label:<9}{value}");
}

/// The `blockers` section: each edge as `<id> (on <op>)`, annotated as a
/// dependency (`on=claim`) or gate (`on=close`) by which transition it gates.
fn blockers(out: &mut String, task: &Task) {
    if task.blockers.is_empty() {
        return;
    }
    out.push_str("  blockers\n");
    for b in &task.blockers {
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
