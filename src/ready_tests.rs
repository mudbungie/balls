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

fn make_child(id: &str, parent: &str, status: Status) -> Task {
    let mut t = make(id, status, vec![]);
    t.parent = Some(parent.into());
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
fn ready_queue_hides_parent_with_live_child() {
    let tasks = vec![
        make("p", Status::Open, vec![]),
        make_child("c1", "p", Status::Open),
    ];
    let ids: Vec<_> = ready_queue(&tasks).iter().map(|t| t.id.clone()).collect();
    assert!(!ids.contains(&"p".to_string()));
    assert!(ids.contains(&"c1".to_string()));
}

#[test]
fn ready_queue_includes_parent_once_children_closed() {
    let tasks = vec![
        make("p", Status::Open, vec![]),
        make_child("c1", "p", Status::Closed),
        make_child("c2", "p", Status::Closed),
    ];
    let ids: Vec<_> = ready_queue(&tasks).iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"p".to_string()));
}

#[test]
fn ready_queue_leaf_unaffected() {
    let tasks = vec![make("a", Status::Open, vec![])];
    let ids: Vec<_> = ready_queue(&tasks).iter().map(|t| t.id.clone()).collect();
    assert_eq!(ids, vec!["a".to_string()]);
}

#[test]
fn live_children_returns_only_non_closed() {
    let tasks = vec![
        make("p", Status::Open, vec![]),
        make_child("c1", "p", Status::Closed),
        make_child("c2", "p", Status::Open),
        make_child("c3", "p", Status::InProgress),
    ];
    let live: Vec<_> = live_children(&tasks, "p").iter().map(|t| t.id.clone()).collect();
    assert!(!live.contains(&"c1".to_string()));
    assert!(live.contains(&"c2".to_string()));
    assert!(live.contains(&"c3".to_string()));
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
    assert!(completion(&tasks, "p").abs() < f64::EPSILON);
}

#[test]
fn missing_dep_treated_as_archived() {
    // Missing deps are treated as closed (archived), not blocked.
    let tasks = vec![make("a", Status::Open, vec!["ghost"])];
    assert!(!is_dep_blocked(&tasks, &tasks[0]));
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
