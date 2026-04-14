//! dep add/rm/tree and link add/rm commands. Split from `lifecycle.rs`
//! to keep both files well under the 300-line cap.

use super::discover;
use crate::cli::{DepCmd, LinkCmd};
use balls::error::{BallError, Result};
use balls::ready;
use balls::store::{task_lock, Store};
use balls::task::{Link, LinkType, Status};

pub fn cmd_dep(sub: DepCmd) -> Result<()> {
    let store = discover()?;
    match sub {
        DepCmd::Add { task, depends_on } => dep_add(&store, task, depends_on),
        DepCmd::Rm { task, depends_on } => dep_rm(&store, task, depends_on),
        DepCmd::Tree { id } => dep_tree(&store, id),
    }
}

fn dep_add(store: &Store, task: String, depends_on: String) -> Result<()> {
    let all = store.all_tasks()?;
    if !all.iter().any(|t| t.id == depends_on) {
        return Err(BallError::TaskNotFound(depends_on));
    }
    if ready::would_create_cycle(&all, &task, &depends_on) {
        return Err(BallError::Cycle(format!(
            "adding {} -> {} would create a cycle",
            task, depends_on
        )));
    }
    {
        let _g = task_lock(store, &task)?;
        let mut t = store.load_task(&task)?;
        if !t.depends_on.contains(&depends_on) {
            t.depends_on.push(depends_on.clone());
            t.touch();
            store.save_task(&t)?;
            store.commit_task(
                &task,
                &format!("balls: dep add {} -> {}", task, depends_on),
            )?;
        }
    }
    println!("{} now depends on {}", task, depends_on);
    Ok(())
}

fn dep_rm(store: &Store, task: String, depends_on: String) -> Result<()> {
    {
        let _g = task_lock(store, &task)?;
        let mut t = store.load_task(&task)?;
        let before = t.depends_on.len();
        t.depends_on.retain(|x| x != &depends_on);
        if t.depends_on.len() != before {
            t.touch();
            store.save_task(&t)?;
            store.commit_task(
                &task,
                &format!("balls: dep rm {} -x {}", task, depends_on),
            )?;
        }
    }
    println!("{} no longer depends on {}", task, depends_on);
    Ok(())
}

fn dep_tree(store: &Store, id: Option<String>) -> Result<()> {
    let tasks = store.all_tasks()?;
    if let Some(id) = id {
        print_tree(&ready::dep_tree(&tasks, &id)?, 0);
    } else {
        use std::collections::HashSet;
        let mut has_dependent: HashSet<String> = HashSet::new();
        for t in &tasks {
            for d in &t.depends_on {
                has_dependent.insert(d.clone());
            }
        }
        for t in &tasks {
            if !has_dependent.contains(&t.id) {
                print_tree(&ready::dep_tree(&tasks, &t.id)?, 0);
            }
        }
    }
    Ok(())
}

fn print_tree(node: &ready::TreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    // Closed tasks are archived out of the tree; catchall covers Open
    // (the only remaining variant) and is the defensive fallback.
    let marker = match node.task.status {
        Status::InProgress => "[~]",
        Status::Review => "[r]",
        Status::Blocked => "[!]",
        Status::Deferred => "[-]",
        _ => "[ ]",
    };
    println!("{}{} {} {}", indent, marker, node.task.id, node.task.title);
    for d in &node.deps {
        print_tree(d, depth + 1);
    }
}

pub fn cmd_link(sub: LinkCmd) -> Result<()> {
    let store = discover()?;
    match sub {
        LinkCmd::Add { task, link_type, target } => link_add(&store, task, link_type, target),
        LinkCmd::Rm { task, link_type, target } => link_rm(&store, task, link_type, target),
    }
}

fn link_add(store: &Store, task: String, link_type: String, target: String) -> Result<()> {
    let lt = LinkType::parse(&link_type)?;
    let all = store.all_tasks()?;
    if !all.iter().any(|t| t.id == target) {
        return Err(BallError::TaskNotFound(target));
    }
    let _g = task_lock(store, &task)?;
    let mut t = store.load_task(&task)?;
    let link = Link { link_type: lt, target: target.clone() };
    if !t.links.contains(&link) {
        t.links.push(link);
        t.touch();
        store.save_task(&t)?;
        store.commit_task(
            &task,
            &format!("balls: link {} {} {}", task, lt.as_str(), target),
        )?;
    }
    println!("{} {} {}", task, lt.as_str(), target);
    Ok(())
}

fn link_rm(store: &Store, task: String, link_type: String, target: String) -> Result<()> {
    let lt = LinkType::parse(&link_type)?;
    let _g = task_lock(store, &task)?;
    let mut t = store.load_task(&task)?;
    let link = Link { link_type: lt, target: target.clone() };
    let before = t.links.len();
    t.links.retain(|l| l != &link);
    if t.links.len() != before {
        t.touch();
        store.save_task(&t)?;
        store.commit_task(
            &task,
            &format!("balls: unlink {} {} {}", task, lt.as_str(), target),
        )?;
    }
    println!("removed {} {} {}", task, lt.as_str(), target);
    Ok(())
}
