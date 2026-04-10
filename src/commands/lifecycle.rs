//! claim, close, drop, update, dep — commands that mutate task state.

use super::{default_identity, discover};
use crate::cli::DepCmd;
use crate::error::{BallError, Result};
use crate::git;
use crate::plugin;
use crate::ready;
use crate::store::task_lock;
use crate::task::{Status, Task, TaskType};
use crate::worktree;
use std::path::PathBuf;

pub fn cmd_claim(id: String, identity: Option<String>) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    let path = worktree::create_worktree(&store, &id, &ident)?;
    let task = store.load_task(&id)?;
    let _ = plugin::run_plugin_push(&store, &task);
    println!("{}", path.display());
    Ok(())
}

pub fn cmd_close(id: String, message: Option<String>) -> Result<()> {
    let store = discover()?;
    let ident = default_identity();
    worktree::close_worktree(&store, &id, message.as_deref(), &ident)?;
    let task = store.load_task(&id)?;
    let _ = plugin::run_plugin_push(&store, &task);
    println!("closed {}", id);
    Ok(())
}

pub fn cmd_drop(id: String, force: bool) -> Result<()> {
    let store = discover()?;
    worktree::drop_worktree(&store, &id, force)?;
    println!("dropped {}", id);
    Ok(())
}

pub fn cmd_update(
    id: String,
    assignments: Vec<String>,
    note: Option<String>,
    identity: Option<String>,
) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);

    let task = {
        let _g = task_lock(&store, &id)?;
        let mut task = store.load_task(&id)?;
        for assign in &assignments {
            let (field, value) = assign.split_once('=').ok_or_else(|| {
                BallError::InvalidTask(format!("expected field=value, got: {}", assign))
            })?;
            apply_field(&mut task, field, value)?;
        }
        if let Some(n) = &note {
            task.append_note(&ident, n);
        }
        task.touch();
        store.save_task(&task)?;
        let rel = PathBuf::from(".ball/tasks").join(format!("{}.json", id));
        git::git_add(&store.root, &[rel.as_path()])?;
        git::git_commit(&store.root, &format!("ball: update {}", id))?;
        task
    };

    let _ = plugin::run_plugin_push(&store, &task);
    println!("updated {}", id);
    Ok(())
}

fn apply_field(task: &mut Task, field: &str, value: &str) -> Result<()> {
    match field {
        "title" => task.title = value.to_string(),
        "priority" => {
            let p: u8 = value
                .parse()
                .map_err(|_| BallError::InvalidTask(format!("priority not integer: {}", value)))?;
            if !(1..=4).contains(&p) {
                return Err(BallError::InvalidTask("priority must be 1..=4".into()));
            }
            task.priority = p;
        }
        "status" => task.status = Status::parse(value)?,
        "type" => task.task_type = TaskType::parse(value)?,
        "parent" => {
            task.parent = if value.is_empty() || value == "null" {
                None
            } else {
                Some(value.to_string())
            };
        }
        "description" => task.description = value.to_string(),
        _ => {
            return Err(BallError::InvalidTask(format!(
                "unknown or unwritable field: {}",
                field
            )));
        }
    }
    Ok(())
}

pub fn cmd_dep(sub: DepCmd) -> Result<()> {
    let store = discover()?;
    match sub {
        DepCmd::Add { task, depends_on } => dep_add(&store, task, depends_on),
        DepCmd::Rm { task, depends_on } => dep_rm(&store, task, depends_on),
        DepCmd::Tree { id } => dep_tree(&store, id),
    }
}

fn dep_add(
    store: &crate::store::Store,
    task: String,
    depends_on: String,
) -> Result<()> {
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
            let rel = PathBuf::from(".ball/tasks").join(format!("{}.json", task));
            git::git_add(&store.root, &[rel.as_path()])?;
            git::git_commit(
                &store.root,
                &format!("ball: dep add {} -> {}", task, depends_on),
            )?;
        }
    }
    println!("{} now depends on {}", task, depends_on);
    Ok(())
}

fn dep_rm(
    store: &crate::store::Store,
    task: String,
    depends_on: String,
) -> Result<()> {
    {
        let _g = task_lock(store, &task)?;
        let mut t = store.load_task(&task)?;
        let before = t.depends_on.len();
        t.depends_on.retain(|x| x != &depends_on);
        if t.depends_on.len() != before {
            t.touch();
            store.save_task(&t)?;
            let rel = PathBuf::from(".ball/tasks").join(format!("{}.json", task));
            git::git_add(&store.root, &[rel.as_path()])?;
            git::git_commit(
                &store.root,
                &format!("ball: dep rm {} -x {}", task, depends_on),
            )?;
        }
    }
    println!("{} no longer depends on {}", task, depends_on);
    Ok(())
}

fn dep_tree(store: &crate::store::Store, id: Option<String>) -> Result<()> {
    let tasks = store.all_tasks()?;
    if let Some(id) = id {
        let tree = ready::dep_tree(&tasks, &id)?;
        print_tree(&tree, 0);
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
                let tree = ready::dep_tree(&tasks, &t.id)?;
                print_tree(&tree, 0);
            }
        }
    }
    Ok(())
}

fn print_tree(node: &ready::TreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let marker = match node.task.status {
        Status::Closed => "[x]",
        Status::InProgress => "[~]",
        Status::Blocked => "[!]",
        Status::Open => "[ ]",
        Status::Deferred => "[-]",
    };
    println!("{}{} {} {}", indent, marker, node.task.id, node.task.title);
    for d in &node.deps {
        print_tree(d, depth + 1);
    }
}
