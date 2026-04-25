use super::*;
use crate::link::{Link, LinkType};
use crate::task::{Status, Task, TaskType};
use chrono::Utc;
use std::collections::BTreeMap;

fn make_task(id: &str) -> Task {
    let now = Utc::now();
    Task {
        id: id.into(),
        title: id.into(),
        task_type: TaskType::task(),
        priority: 3,
        status: Status::Open,
        parent: None,
        depends_on: vec![],
        description: String::new(),
        created_at: now,
        updated_at: now,
        closed_at: None,
        claimed_by: None,
        branch: None,
        tags: vec![],
        notes: vec![],
        links: vec![],
        closed_children: vec![],
        external: BTreeMap::new(),
        synced_at: BTreeMap::new(),
        delivered_in: None,
        extra: BTreeMap::new(),
    }
}

// ---------- claimed_badge ----------

#[test]
fn claimed_badge_none_is_empty() {
    let t = make_task("bl-a");
    assert_eq!(Display::plain().claimed_badge(&t, "me"), "");
    assert_eq!(Display::styled().claimed_badge(&t, "me"), "");
}

#[test]
fn claimed_badge_other_is_empty() {
    let mut t = make_task("bl-a");
    t.claimed_by = Some("someone_else".into());
    assert_eq!(Display::styled().claimed_badge(&t, "me"), "");
}

#[test]
fn claimed_badge_self_renders_star() {
    let mut t = make_task("bl-a");
    t.claimed_by = Some("me".into());
    assert_eq!(Display::plain().claimed_badge(&t, "me"), "*");
    assert_eq!(Display::styled().claimed_badge(&t, "me"), "★");
}

// ---------- deps_badge ----------

#[test]
fn deps_badge_no_deps_empty() {
    let t = make_task("bl-a");
    assert_eq!(Display::styled().deps_badge(&t, std::slice::from_ref(&t)), "");
}

#[test]
fn deps_badge_all_closed_is_empty() {
    let mut t = make_task("bl-a");
    t.depends_on = vec!["bl-dep".into()];
    let mut dep = make_task("bl-dep");
    dep.status = Status::Closed;
    assert_eq!(Display::styled().deps_badge(&t, &[t.clone(), dep]), "");
}

#[test]
fn deps_badge_open_dep_renders_both_flavors() {
    let mut t = make_task("bl-a");
    t.depends_on = vec!["bl-dep".into()];
    let dep = make_task("bl-dep"); // Open by default
    let all = vec![t.clone(), dep];
    assert_eq!(Display::plain().deps_badge(&t, &all), "D");
    assert_eq!(Display::styled().deps_badge(&t, &all), "◆");
}

// ---------- gates_badge ----------

#[test]
fn gates_badge_no_links_empty() {
    let t = make_task("bl-a");
    assert_eq!(Display::styled().gates_badge(&t, &[]), "");
}

#[test]
fn gates_badge_non_gates_link_empty() {
    let mut t = make_task("bl-a");
    t.links = vec![Link {
        link_type: LinkType::RelatesTo,
        target: "bl-x".into(),
        extra: std::collections::BTreeMap::new(),
    }];
    assert_eq!(Display::styled().gates_badge(&t, &[]), "");
}

#[test]
fn gates_badge_closed_target_empty() {
    let mut t = make_task("bl-a");
    t.links = vec![Link {
        link_type: LinkType::Gates,
        target: "bl-g".into(),
        extra: std::collections::BTreeMap::new(),
    }];
    let mut g = make_task("bl-g");
    g.status = Status::Closed;
    assert_eq!(Display::styled().gates_badge(&t, &[g]), "");
}

#[test]
fn gates_badge_open_target_renders_both_flavors() {
    let mut t = make_task("bl-a");
    t.links = vec![Link {
        link_type: LinkType::Gates,
        target: "bl-g".into(),
        extra: std::collections::BTreeMap::new(),
    }];
    let all = vec![make_task("bl-g")]; // Open
    assert_eq!(Display::plain().gates_badge(&t, &all), "G");
    assert_eq!(Display::styled().gates_badge(&t, &all), "⛓");
}
