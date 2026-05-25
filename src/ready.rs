use crate::error::{BallError, Result};
use crate::task::{Status, Task};
use std::collections::{HashMap, HashSet};

/// Return tasks that are ready to be claimed.
///
/// Filters: must be `Open`, unclaimed, all `depends_on` closed/archived, and
/// — if a parent — every child closed. A parent with even one live child is
/// not claimable (the children are the real work), so it stays out of `ready`
/// until the last child closes, at which point `bl close <parent>` is the
/// remaining action.
pub fn ready_queue(tasks: &[Task]) -> Vec<&Task> {
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let parents_with_live_child: HashSet<&str> = tasks
        .iter()
        .filter(|t| t.status != Status::Closed)
        .filter_map(|t| t.parent.as_deref())
        .collect();
    let mut out: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status == Status::Open)
        .filter(|t| t.claimed_by.is_none())
        .filter(|t| !parents_with_live_child.contains(t.id.as_str()))
        .filter(|t| {
            t.depends_on.iter().all(|d| {
                // missing dep = archived = closed
                by_id.get(d.as_str()).is_none_or(|dep| dep.status == Status::Closed)
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
    out
}

/// Returns true if a task is dependency-blocked (not same as Status::Blocked).
/// Missing deps are treated as closed (archived tasks are deleted from HEAD).
pub fn is_dep_blocked(tasks: &[Task], task: &Task) -> bool {
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    task.depends_on.iter().any(|d| {
        // missing dep = archived = closed
        by_id.get(d.as_str()).is_some_and(|dep| dep.status != Status::Closed)
    })
}

pub fn children_of<'a>(tasks: &'a [Task], parent_id: &str) -> Vec<&'a Task> {
    tasks
        .iter()
        .filter(|t| t.parent.as_deref() == Some(parent_id))
        .collect()
}

/// Non-closed children of `parent_id`. Empty when the parent has no
/// children or all of them are closed — i.e. when `bl claim <parent>` is
/// the next legitimate action.
pub fn live_children<'a>(tasks: &'a [Task], parent_id: &str) -> Vec<&'a Task> {
    children_of(tasks, parent_id)
        .into_iter()
        .filter(|t| t.status != Status::Closed)
        .collect()
}

pub fn completion(tasks: &[Task], parent_id: &str) -> f64 {
    let parent = tasks.iter().find(|t| t.id == parent_id);
    let archived = parent.map_or(0, |p| p.closed_children.len());
    let live = children_of(tasks, parent_id);
    let live_closed = live.iter().filter(|t| t.status == Status::Closed).count();
    let total = archived + live.len();
    if total == 0 {
        return 0.0;
    }
    (archived + live_closed) as f64 / total as f64
}

/// Adding `from` -> `to` (i.e. `from` depends on `to`). Returns true if that would create a cycle.
pub fn would_create_cycle(tasks: &[Task], from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    // DFS from `to` along depends_on edges; if we ever reach `from`, there's a cycle.
    let mut stack = vec![to];
    let mut seen: HashSet<&str> = HashSet::new();
    while let Some(cur) = stack.pop() {
        if cur == from {
            return true;
        }
        if !seen.insert(cur) {
            continue;
        }
        if let Some(t) = by_id.get(cur) {
            for d in &t.depends_on {
                stack.push(d.as_str());
            }
        }
    }
    false
}

pub fn validate_deps(tasks: &[Task], deps: &[String]) -> Result<()> {
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    for d in deps {
        if !by_id.contains_key(d.as_str()) {
            return Err(BallError::InvalidTask(format!(
                "dependency does not exist: {d}"
            )));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct TreeNode<'a> {
    pub task: &'a Task,
    pub deps: Vec<TreeNode<'a>>,
}

pub fn dep_tree<'a>(tasks: &'a [Task], root_id: &str) -> Result<TreeNode<'a>> {
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut seen = HashSet::new();
    build_node(&by_id, root_id, &mut seen)
}

fn build_node<'a>(
    by_id: &HashMap<&str, &'a Task>,
    id: &str,
    seen: &mut HashSet<String>,
) -> Result<TreeNode<'a>> {
    let task = by_id
        .get(id)
        .copied()
        .ok_or_else(|| BallError::TaskNotFound(id.to_string()))?;
    let mut deps = Vec::new();
    if seen.insert(id.to_string()) {
        for d in &task.depends_on {
            if let Ok(node) = build_node(by_id, d, seen) {
                deps.push(node);
            }
        }
    }
    Ok(TreeNode { task, deps })
}

#[cfg(test)]
#[path = "ready_tests.rs"]
mod tests;
