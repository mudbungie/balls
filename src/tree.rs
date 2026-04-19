//! Parent-edge tree rendering for `bl dep tree`.
//!
//! `bl dep tree` is a misnomer kept for back-compat — the tree is the
//! parent/child hierarchy. Dep edges and gates render as inline
//! annotations on each row, never as nesting. Cycles in parent edges
//! shouldn't happen in healthy data; we still defend against them so
//! a corrupt repo doesn't loop the renderer.

use crate::display::Display;
use crate::task::{LinkType, Status, Task, TaskType};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Write;

/// One node in the parent-hierarchy tree.
#[derive(Debug)]
pub struct Node<'a> {
    pub task: &'a Task,
    pub children: Vec<Node<'a>>,
    /// True when this id was already on the ancestor stack — descent
    /// stops so the renderer can mark the row and move on.
    pub cycle: bool,
}

/// Stable JSON shape: nested children rather than the flat task list
/// so a consumer can walk the tree without re-indexing.
#[derive(Serialize)]
pub struct JsonNode<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub status: &'a str,
    pub children: Vec<JsonNode<'a>>,
}

impl<'a> JsonNode<'a> {
    pub fn from_node(n: &Node<'a>) -> JsonNode<'a> {
        JsonNode {
            id: &n.task.id,
            title: &n.task.title,
            status: n.task.status.as_str(),
            children: n.children.iter().map(JsonNode::from_node).collect(),
        }
    }
}

/// Build the forest: every parentless task becomes a root, sorted by
/// id for deterministic output.
pub fn forest(tasks: &[Task]) -> Vec<Node<'_>> {
    let mut roots: Vec<&Task> = tasks.iter().filter(|t| t.parent.is_none()).collect();
    roots.sort_by(|a, b| a.id.cmp(&b.id));
    let by_parent = build_index(tasks);
    roots
        .into_iter()
        .map(|r| build(&by_parent, r, &mut Vec::new()))
        .collect()
}

/// Build a single subtree rooted at `root_id`. Returns `None` if the
/// id isn't in the task set.
pub fn rooted<'a>(tasks: &'a [Task], root_id: &str) -> Option<Node<'a>> {
    let root = tasks.iter().find(|t| t.id == root_id)?;
    let by_parent = build_index(tasks);
    Some(build(&by_parent, root, &mut Vec::new()))
}

fn build_index(tasks: &[Task]) -> HashMap<&str, Vec<&Task>> {
    let mut map: HashMap<&str, Vec<&Task>> = HashMap::new();
    for t in tasks {
        if let Some(p) = &t.parent {
            map.entry(p.as_str()).or_default().push(t);
        }
    }
    for v in map.values_mut() {
        v.sort_by(|a, b| a.id.cmp(&b.id));
    }
    map
}

fn build<'a>(
    by_parent: &HashMap<&str, Vec<&'a Task>>,
    node: &'a Task,
    ancestors: &mut Vec<String>,
) -> Node<'a> {
    if ancestors.iter().any(|a| a == &node.id) {
        return Node { task: node, children: Vec::new(), cycle: true };
    }
    ancestors.push(node.id.clone());
    let kids = by_parent.get(node.id.as_str()).cloned().unwrap_or_default();
    let children: Vec<Node<'a>> = kids
        .into_iter()
        .map(|c| build(by_parent, c, ancestors))
        .collect();
    ancestors.pop();
    Node { task: node, children, cycle: false }
}

/// Render a forest to a string. Roots print sequentially with a
/// blank line between them so the eye separates independent trees.
/// Caller decides where to send the result (stdout, test buffer).
pub fn render_forest(roots: &[Node], all: &[Task], d: Display) -> String {
    let mut buf = String::new();
    for (i, r) in roots.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }
        render_tree(r, all, d, 0, true, &[], &mut buf);
    }
    buf
}

fn render_tree(
    node: &Node,
    all: &[Task],
    d: Display,
    depth: usize,
    is_last: bool,
    ancestors: &[bool],
    buf: &mut String,
) {
    buf.push_str(&d.tree_prefix(depth, is_last, ancestors));
    buf.push_str(&format_line(node, all, d));
    buf.push('\n');
    if node.cycle {
        return;
    }
    let mut new_anc: Vec<bool> = ancestors.to_vec();
    if depth > 0 {
        new_anc.push(is_last);
    }
    let n = node.children.len();
    for (i, c) in node.children.iter().enumerate() {
        render_tree(c, all, d, depth + 1, i + 1 == n, &new_anc, buf);
    }
}

fn format_line(node: &Node, all: &[Task], d: Display) -> String {
    let t = node.task;
    let mut out = format!("{}  {}", t.id, t.title);
    if matches!(t.task_type, TaskType::Epic) {
        out.push_str("  [epic]");
    }
    out.push_str("  ");
    out.push_str(d.status_glyph(&t.status));
    out.push(' ');
    out.push_str(&d.status_word(&t.status));
    let blocked = blockers(t, all);
    if !blocked.is_empty() {
        let glyph = if d.use_unicode() { "⌀" } else { "[!]" };
        let _ = write!(out, "  {glyph} blocked by {}", blocked.join(", "));
    }
    if gates_parent(t, all) {
        let glyph = if d.use_unicode() { "⛓" } else { "[G]" };
        let _ = write!(out, "  {glyph} gates parent");
    }
    if node.cycle {
        let arrow = if d.use_unicode() { "↺" } else { "<-" };
        let _ = write!(out, "  ({arrow} cycle)");
    }
    out
}

fn blockers(t: &Task, all: &[Task]) -> Vec<String> {
    t.depends_on
        .iter()
        .filter(|d| {
            all.iter()
                .any(|o| &o.id == *d && !matches!(o.status, Status::Closed))
        })
        .cloned()
        .collect()
}

fn gates_parent(t: &Task, all: &[Task]) -> bool {
    let Some(parent_id) = &t.parent else { return false };
    all.iter()
        .find(|o| &o.id == parent_id)
        .is_some_and(|p| {
            p.links
                .iter()
                .any(|l| matches!(l.link_type, LinkType::Gates) && l.target == t.id)
        })
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
