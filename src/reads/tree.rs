//! The containment forest a human `bl list` renders (bl-61e0) — the read-side
//! projection of [`crate::task::Task::parent`].
//!
//! Containment is already stored frontmatter and already rendered by `show`'s
//! children section; a flat listing simply DISCARDS it, which is what makes a
//! store full of identically-titled gate children unreadable. So the tree is
//! derived, human-render only — the same class as the claim-age suffix, the
//! fleet-view label and the `->bl-xxxx` delivery marker ([`super::target`]).
//! No new stored field and no new flag: bedrock `--json` returns before any of
//! this and stays the flat stored-frontmatter mirror (§3).
//!
//! ONE invariant, no special cases: the forest is over the RENDERED set. A row
//! indents under its parent only when that parent is also being rendered;
//! otherwise it IS a root. A closed parent, a filtered-out parent, a foreign
//! parent under the default scope — every one of them takes that same path, so
//! "orphan" needs no branch of its own.
//!
//! §10's display order is not re-derived here: the caller hands rows in §10
//! order and the walk is stable, so that order simply applies PER SIBLING LEVEL
//! instead of globally. Ordering is display-only (it never enters `ready()`,
//! §3/§10), so nothing semantic moves — the accepted trade is that a
//! low-priority parent pulls its high-priority children down with it, which is
//! right: a child is meaningless out of its parent's context.

use std::collections::HashMap;

/// The render order of `nodes` — each `(id, parent)` in §10 order — as
/// `(index, depth)` pairs, parents immediately followed by their subtree.
/// TOTAL: every input row comes back exactly once, at the depth of its chain of
/// rendered ancestors.
pub(crate) fn forest(nodes: &[(&str, Option<&str>)]) -> Vec<(usize, usize)> {
    let at: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, (id, _))| (*id, i)).collect();
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut roots: Vec<usize> = Vec::new();
    for (i, (_, parent)) in nodes.iter().enumerate() {
        // The invariant in one expression: a parent that is not rendered here
        // (or is the row itself) leaves the row parentless, hence a root.
        match parent.and_then(|p| at.get(p)).copied().filter(|&p| p != i) {
            Some(p) => kids[p].push(i),
            None => roots.push(i),
        }
    }
    let mut out = Vec::with_capacity(nodes.len());
    let mut seen = vec![false; nodes.len()];
    // The roots in order, THEN every row: the second sweep is the cycle guard's
    // other half. A parent cycle has no root at all, so its members would
    // otherwise never be reached; visiting them as roots renders them, and the
    // `seen` set that makes the sweep a no-op for the already-rendered is the
    // same set that terminates the walk inside the cycle. The store invariant
    // is not trusted — the render is total either way.
    for i in roots.into_iter().chain(0..nodes.len()) {
        visit(i, 0, &kids, &mut seen, &mut out);
    }
    out
}

/// Emit `i` at `depth`, then its children one level deeper — skipping any row
/// already emitted, which both dedupes the two sweeps and terminates a cycle.
fn visit(i: usize, depth: usize, kids: &[Vec<usize>], seen: &mut [bool], out: &mut Vec<(usize, usize)>) {
    if std::mem::replace(&mut seen[i], true) {
        return;
    }
    out.push((i, depth));
    for &k in &kids[i] {
        visit(k, depth + 1, kids, seen, out);
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
