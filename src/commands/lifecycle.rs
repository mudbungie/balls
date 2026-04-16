//! claim, review, close, drop, update — commands that mutate a task's
//! own lifecycle. Dep and link graph operations live in `dep_link.rs`.

use super::{default_identity, discover};
use balls::error::{BallError, Result};
use balls::plugin;
use balls::store::task_lock;
use balls::task::{Status, Task, TaskType};
use balls::{task_io, worktree};

pub fn cmd_claim(id: String, identity: Option<String>, no_worktree: bool) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    if store.no_git && !no_worktree {
        return Err(BallError::Other(
            "no git repo: use `bl claim --no-worktree` to claim without a worktree".into(),
        ));
    }
    if no_worktree {
        worktree::claim_no_worktree(&store, &id, &ident)?;
        println!("claimed {id} (no worktree)");
    } else {
        let path = worktree::create_worktree(&store, &id, &ident)?;
        let task = store.load_task(&id)?;
        if let Ok(results) = plugin::run_plugin_push(&store, &task) {
            let _ = plugin::apply_push_response(&store, &id, &results);
            let main_branch = balls::git::git_current_branch(&store.root)?;
            let _ = balls::git::git_merge(&path, &main_branch);
        }
        println!("{}", path.display());
    }
    Ok(())
}

pub fn cmd_review(id: String, message: Option<String>) -> Result<()> {
    let store = discover()?;
    let ident = default_identity();
    if store.no_git {
        balls::review::review_no_git(&store, &id, message.as_deref(), &ident)?;
    } else {
        balls::review::review_worktree(&store, &id, message.as_deref(), &ident)?;
    }
    let task = store.load_task(&id)?;
    if let Ok(results) = plugin::run_plugin_push(&store, &task) {
        let _ = plugin::apply_push_response(&store, &id, &results);
    }
    println!("reviewed {id} — from the repo root, run `bl close {id} -m \"...\"` to finish");
    Ok(())
}

pub fn cmd_close(id: String, message: Option<String>) -> Result<()> {
    let store = discover()?;
    let ident = default_identity();
    let task = if store.no_git {
        balls::review::close_no_git(&store, &id, message.as_deref(), &ident)?
    } else {
        balls::review::close_worktree(&store, &id, message.as_deref(), &ident)?
    };
    let _ = plugin::run_plugin_push(&store, &task);
    println!("closed {id}");
    if !store.no_git {
        println!("{}", store.root.display());
    }
    Ok(())
}

pub fn cmd_drop(id: String, force: bool) -> Result<()> {
    let store = discover()?;
    if store.no_git {
        worktree::drop_no_worktree(&store, &id)?;
    } else {
        worktree::drop_worktree(&store, &id, force)?;
    }
    println!("dropped {id}");
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
        if closing {
            balls::review::enforce_gates(&store, &task)?;
        }
        for assign in &assignments {
            let (field, value) = assign.split_once('=').ok_or_else(|| {
                BallError::InvalidTask(format!("expected field=value, got: {assign}"))
            })?;
            apply_field(&mut task, field, value)?;
        }
        if closing {
            task.closed_at = Some(chrono::Utc::now());
        }
        task.touch();
        if closing {
            // Close + archive is one atomic state-branch commit. The
            // reviewer's note is embedded in the commit message so it
            // survives the git-rm of the notes file.
            let msg = match &note {
                Some(n) => format!("balls: close {} - {}\n\n{}", id, task.title, n),
                None => format!("balls: close {} - {}", id, task.title),
            };
            let _ = &ident; // ident not used on the close path
            store.close_and_archive(&task, &msg)?;
        } else {
            store.save_task(&task)?;
            if let Some(n) = &note {
                task_io::append_note_to(&store.task_path(&id)?, &ident, n)?;
            }
            store.commit_task(&id, &format!("balls: update {} - {}", id, task.title))?;
        }
        task
    };

    if let Ok(results) = plugin::run_plugin_push(&store, &task) {
        let _ = plugin::apply_push_response(&store, &id, &results);
    }

    if closing {
        println!("closed and archived {id}");
    } else {
        println!("updated {id}");
    }
    Ok(())
}

fn apply_field(task: &mut Task, field: &str, value: &str) -> Result<()> {
    match field {
        "title" => task.title = value.to_string(),
        "priority" => task.priority = balls::task::parse_priority(value)?,
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
                "unknown or unwritable field: {field}"
            )));
        }
    }
    Ok(())
}
