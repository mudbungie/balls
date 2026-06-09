//! `bl dep-tree` — the containment forest (§3/§10). Roots are the balls with no
//! `parent` (or a `parent` pointing outside the live set); each node nests its
//! containment children, annotated with its `blockers` edges — a dependency
//! (`on=claim`) reads `needs <id>`, a gate (`on=close`) reads `gate <id>`.
//!
//! Containment is a forest: every ball has at most one `parent`, so the
//! child-of relation reachable from a root is acyclic — no visited-set guard is
//! needed. A ball trapped in a `parent` cycle simply has no root and does not
//! print; a blocker cycle is inert (readiness is immediate-only, §10) and shows
//! only as the inline `needs`/`gate` annotations, not as anything this view
//! walks.

use std::fmt::Write;

use serde_json::Value;

use super::{json_line, task_json, Catalog, Entry, Flags, Style};
use crate::task::On;

/// Render `bl dep-tree`. `--json` is the nested forest of node objects;
/// otherwise an indented badge tree.
pub(crate) fn render(cat: &Catalog, flags: &Flags, style: &Style) -> String {
    if flags.json {
        // The nesting is derived, so `--json` is the FLAT bedrock array (§9) —
        // every ball's stored `parent` is in its record; the consumer rebuilds
        // the tree. The forest shape is the human projection alone.
        let arr: Vec<Value> = cat.entries().iter().map(|e| task_json(&e.id, &e.task)).collect();
        return json_line(&Value::Array(arr));
    }
    let mut out = String::new();
    for root in &roots(cat) {
        walk(cat, root, 0, style, &mut out);
    }
    out
}

/// The forest roots: balls with no `parent`, or whose `parent` names no live
/// ball (a dangling pointer is display-only, never corruption — §3).
fn roots(cat: &Catalog) -> Vec<&Entry> {
    cat.entries()
        .iter()
        .filter(|e| match &e.task.parent {
            None => true,
            Some(p) => cat.get(p).is_none(),
        })
        .collect()
}

/// The containment children of `id`, in catalog (id) order.
fn children<'a>(cat: &'a Catalog, id: &str) -> Vec<&'a Entry> {
    cat.entries()
        .iter()
        .filter(|c| c.task.parent.as_deref() == Some(id))
        .collect()
}

/// One human node line at `depth` indentation, then its children recursively.
fn walk(cat: &Catalog, e: &Entry, depth: usize, style: &Style, out: &mut String) {
    let indent = "  ".repeat(depth);
    let _ = writeln!(
        out,
        "{indent}{} {}  {}{}",
        style.badge(cat.status(e)),
        e.id,
        e.task.title,
        annotation(e),
    );
    for child in children(cat, &e.id) {
        walk(cat, child, depth + 1, style, out);
    }
}

/// The inline blocker annotation for a node — ` [needs A, gate B]`, or empty
/// when the ball has no edges. `needs` is a claim-blocker (dependency), `gate` a
/// close-blocker; an edge on any OTHER op (§10/§15) is labelled by its op token.
fn annotation(e: &Entry) -> String {
    if e.task.blockers.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = e
        .task
        .blockers
        .iter()
        .map(|b| {
            let kind = match b.on {
                On::Claim => "needs",
                On::Close => "gate",
                other => other.token(),
            };
            format!("{kind} {}", b.id)
        })
        .collect();
    format!(" [{}]", parts.join(", "))
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
