//! dep add/rm/tree and link add/rm commands. Split from `lifecycle.rs`
//! to keep both files well under the 300-line cap.

use super::discover;
use crate::cli::{DepCmd, LinkCmd};
use balls::display;
use balls::error::{BallError, Result};
use balls::ready;
use balls::store::{task_lock, Store};
use balls::task::{Link, LinkType};
use balls::tree;

pub fn cmd_dep(sub: DepCmd) -> Result<()> {
    let store = discover()?;
    match sub {
        DepCmd::Add { task, depends_on } => dep_add(&store, task, depends_on),
        DepCmd::Rm { task, depends_on } => dep_rm(&store, task, depends_on),
        DepCmd::Tree { id, json } => dep_tree(&store, id, json),
    }
}

fn dep_add(store: &Store, task: String, depends_on: String) -> Result<()> {
    let all = store.all_tasks()?;
    if !all.iter().any(|t| t.id == depends_on) {
        return Err(BallError::TaskNotFound(depends_on));
    }
    if ready::would_create_cycle(&all, &task, &depends_on) {
        return Err(BallError::Cycle(format!(
            "adding {task} -> {depends_on} would create a cycle"
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
                &format!("balls: dep add {task} -> {depends_on}"),
            )?;
        }
    }
    println!("{task} now depends on {depends_on}");
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
                &format!("balls: dep rm {task} -x {depends_on}"),
            )?;
        }
    }
    println!("{task} no longer depends on {depends_on}");
    Ok(())
}

fn dep_tree(store: &Store, id: Option<String>, json: bool) -> Result<()> {
    let tasks = store.all_tasks()?;
    let roots = if let Some(id) = id {
        let n = tree::rooted(&tasks, &id).ok_or(BallError::TaskNotFound(id))?;
        vec![n]
    } else {
        tree::forest(&tasks)
    };
    if json {
        let payload: Vec<tree::JsonNode> = roots.iter().map(tree::JsonNode::from_node).collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print!("{}", tree::render_forest(&roots, &tasks, display::global()));
    }
    Ok(())
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
    let lt_str = lt.as_str().to_string();
    let _g = task_lock(store, &task)?;
    let mut t = store.load_task(&task)?;
    let link = Link { link_type: lt, target: target.clone() };
    if !t.links.contains(&link) {
        t.links.push(link);
        t.touch();
        store.save_task(&t)?;
        store.commit_task(
            &task,
            &format!("balls: link {task} {lt_str} {target}"),
        )?;
    }
    println!("{task} {lt_str} {target}");
    Ok(())
}

fn link_rm(store: &Store, task: String, link_type: String, target: String) -> Result<()> {
    let lt = LinkType::parse(&link_type)?;
    let lt_str = lt.as_str().to_string();
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
            &format!("balls: unlink {task} {lt_str} {target}"),
        )?;
    }
    println!("removed {task} {lt_str} {target}");
    Ok(())
}
