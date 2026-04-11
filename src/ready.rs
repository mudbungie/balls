use crate::error::{BallError, Result};
use crate::task::{Status, Task};
use std::collections::{HashMap, HashSet};

/// Return tasks that are ready to be claimed.
pub fn ready_queue(tasks: &[Task]) -> Vec<&Task> {
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut out: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status == Status::Open)
        .filter(|t| t.claimed_by.is_none())
        .filter(|t| {
            t.depends_on.iter().all(|d| {
                by_id
                    .get(d.as_str())
                    .map(|dep| dep.status == Status::Closed)
                    .unwrap_or(false)
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
pub fn is_dep_blocked(tasks: &[Task], task: &Task) -> bool {
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    task.depends_on.iter().any(|d| {
        by_id
            .get(d.as_str())
            .map(|dep| dep.status != Status::Closed)
            .unwrap_or(true) // unknown dep = blocked
    })
}

pub fn children_of<'a>(tasks: &'a [Task], parent_id: &str) -> Vec<&'a Task> {
    tasks
        .iter()
        .filter(|t| t.parent.as_deref() == Some(parent_id))
        .collect()
}

pub fn completion(tasks: &[Task], parent_id: &str) -> f64 {
    let kids = children_of(tasks, parent_id);
    if kids.is_empty() {
        return 0.0;
    }
    let closed = kids.iter().filter(|t| t.status == Status::Closed).count();
    closed as f64 / kids.len() as f64
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
                "dependency does not exist: {}",
                d
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
mod tests {
    use super::*;
    use crate::task::{NewTaskOpts, Task};

    fn make(id: &str, status: Status, deps: Vec<&str>) -> Task {
        let mut t = Task::new(
            NewTaskOpts {
                title: id.into(),
                depends_on: deps.into_iter().map(String::from).collect(),
                ..Default::default()
            },
            id.into(),
        );
        t.status = status;
        t
    }

    #[test]
    fn ready_queue_filters() {
        let tasks = vec![
            make("a", Status::Open, vec![]),
            make("b", Status::Closed, vec![]),
            make("c", Status::Open, vec!["b"]),
            make("d", Status::Open, vec!["a"]),
        ];
        let ready = ready_queue(&tasks);
        let ids: Vec<_> = ready.iter().map(|t| t.id.clone()).collect();
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"c".to_string()));
        assert!(!ids.contains(&"d".to_string())); // a is not closed
        assert!(!ids.contains(&"b".to_string())); // closed
    }

    #[test]
    fn cycle_detection_self() {
        let tasks = vec![make("a", Status::Open, vec![])];
        assert!(would_create_cycle(&tasks, "a", "a"));
    }

    #[test]
    fn cycle_detection_transitive() {
        let tasks = vec![
            make("a", Status::Open, vec!["b"]),
            make("b", Status::Open, vec!["c"]),
            make("c", Status::Open, vec![]),
        ];
        // Adding c -> a would create a cycle
        assert!(would_create_cycle(&tasks, "c", "a"));
        // Adding c -> b exists already but no cycle created by repetition
        assert!(!would_create_cycle(&tasks, "a", "c"));
    }

    #[test]
    fn completion_parent() {
        let mut tasks = vec![make("p", Status::Open, vec![])];
        let mut c1 = make("c1", Status::Closed, vec![]);
        c1.parent = Some("p".into());
        let mut c2 = make("c2", Status::Open, vec![]);
        c2.parent = Some("p".into());
        tasks.push(c1);
        tasks.push(c2);
        assert!((completion(&tasks, "p") - 0.5).abs() < 1e-9);
    }

    #[test]
    fn completion_no_children_is_zero() {
        let tasks = vec![make("p", Status::Open, vec![])];
        assert_eq!(completion(&tasks, "p"), 0.0);
    }

    #[test]
    fn is_dep_blocked_unknown_dep_is_blocked() {
        let tasks = vec![make("a", Status::Open, vec!["ghost"])];
        assert!(is_dep_blocked(&tasks, &tasks[0]));
    }

    #[test]
    fn validate_deps_missing_errors() {
        let tasks = vec![make("a", Status::Open, vec![])];
        let err = validate_deps(&tasks, &["ghost".to_string()]).unwrap_err();
        assert!(matches!(err, crate::error::BallError::InvalidTask(_)));
    }

    #[test]
    fn validate_deps_present_ok() {
        let tasks = vec![make("a", Status::Open, vec![])];
        validate_deps(&tasks, &["a".to_string()]).unwrap();
    }

    #[test]
    fn cycle_detection_dfs_revisits_diamond_node() {
        // a depends on b and c; both b and c depend on d. Traversing from 'a'
        // along depends_on edges pushes d twice; the second pop must hit the
        // `continue` branch in the DFS visited set.
        let tasks = vec![
            make("a", Status::Open, vec!["b", "c"]),
            make("b", Status::Open, vec!["d"]),
            make("c", Status::Open, vec!["d"]),
            make("d", Status::Open, vec![]),
        ];
        // No cycle; exercises the visited branch regardless.
        assert!(!would_create_cycle(&tasks, "nonexistent", "a"));
    }

    #[test]
    fn cycle_detection_diamond_ok() {
        // a -> b, a -> c, b -> d, c -> d  — no cycle already
        let tasks = vec![
            make("a", Status::Open, vec!["b", "c"]),
            make("b", Status::Open, vec!["d"]),
            make("c", Status::Open, vec!["d"]),
            make("d", Status::Open, vec![]),
        ];
        // Adding a -> d doesn't create a cycle (already transitively present).
        assert!(!would_create_cycle(&tasks, "a", "d"));
        // Adding b -> a WOULD create a cycle (b -> a -> b).
        assert!(would_create_cycle(&tasks, "b", "a"));
    }

    #[test]
    fn dep_tree_missing_node_errors() {
        let tasks = vec![make("a", Status::Open, vec![])];
        assert!(dep_tree(&tasks, "nonexistent").is_err());
    }

    #[test]
    fn dep_tree_builds() {
        let tasks = vec![
            make("a", Status::Open, vec!["b"]),
            make("b", Status::Closed, vec![]),
        ];
        let tree = dep_tree(&tasks, "a").unwrap();
        assert_eq!(tree.task.id, "a");
        assert_eq!(tree.deps.len(), 1);
        assert_eq!(tree.deps[0].task.id, "b");
    }

    #[test]
    fn children_of_returns_matching() {
        let mut tasks = vec![make("p", Status::Open, vec![])];
        let mut c = make("c", Status::Open, vec![]);
        c.parent = Some("p".into());
        tasks.push(c);
        assert_eq!(children_of(&tasks, "p").len(), 1);
        assert_eq!(children_of(&tasks, "nobody").len(), 0);
    }
}
