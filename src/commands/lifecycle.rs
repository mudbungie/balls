//! claim, review, close, drop, update — commands that mutate a task's
//! own lifecycle. Dep and link graph operations live in `dep_link.rs`.

use super::{default_identity, discover};
use balls::error::{BallError, Result};
use balls::plugin;
use balls::store::task_lock;
use balls::task::{Status, Task, TaskType};
use balls::{task_io, worktree};

pub fn cmd_claim(id: String, identity: Option<String>) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    let path = worktree::create_worktree(&store, &id, &ident)?;
    let task = store.load_task(&id)?;
    if let Ok(results) = plugin::run_plugin_push(&store, &task) {
        let _ = plugin::apply_push_response(&store, &id, &results);
        // Plugin response committed to state branch after worktree
        // creation — merge main into worktree to keep it current.
        let main_branch = balls::git::git_current_branch(&store.root)?;
        let _ = balls::git::git_merge(&path, &main_branch);
    }
    println!("{}", path.display());
    Ok(())
}

pub fn cmd_review(id: String, message: Option<String>) -> Result<()> {
    let store = discover()?;
    let ident = default_identity();
    balls::review::review_worktree(&store, &id, message.as_deref(), &ident)?;
    let task = store.load_task(&id)?;
    if let Ok(results) = plugin::run_plugin_push(&store, &task) {
        let _ = plugin::apply_push_response(&store, &id, &results);
    }
    println!("submitted {} for review", id);
    Ok(())
}

pub fn cmd_close(id: String, message: Option<String>) -> Result<()> {
    let store = discover()?;
    let ident = default_identity();
    let task = balls::review::close_worktree(&store, &id, message.as_deref(), &ident)?;
    let _ = plugin::run_plugin_push(&store, &task);
    println!("closed {}", id);
    println!("{}", store.root.display());
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
    let closing = assignments.iter().any(|a| a == "status=closed");
    let task = {
        let _g = task_lock(&store, &id)?;
        let mut task = store.load_task(&id)?;
        if closing && task.claimed_by.is_some() {
            return Err(BallError::InvalidTask(
                "use `bl close` for claimed tasks (handles worktree teardown and merge)".into(),
            ));
        }
        for assign in &assignments {
            let (field, value) = assign.split_once('=').ok_or_else(|| {
                BallError::InvalidTask(format!("expected field=value, got: {}", assign))
            })?;
            apply_field(&mut task, field, value)?;
        }
        if closing {
            task.closed_at = Some(chrono::Utc::now());
        }
        task.touch();
        store.save_task(&task)?;
        if let Some(n) = &note {
            task_io::append_note_to(&store.task_path(&id)?, &ident, n)?;
        }
        if closing {
            balls::review::archive_task(&store, &task)?;
            store.commit_staged(&format!("balls: close {} - {}", id, task.title))?;
        } else {
            store.commit_task(&id, &format!("balls: update {} - {}", id, task.title))?;
        }
        task
    };

    if let Ok(results) = plugin::run_plugin_push(&store, &task) {
        let _ = plugin::apply_push_response(&store, &id, &results);
    }

    if closing {
        println!("closed and archived {}", id);
    } else {
        println!("updated {}", id);
    }
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
