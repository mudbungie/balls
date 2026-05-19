//! `bl update` — field/status edits and the multi-agent reject path.
//! Split out of `lifecycle.rs` (which was at the 300-line cap) so the
//! deferred-mode reject leg (SPEC §7.3) has room to live next to it.

use super::{default_identity, discover};
use balls::error::{BallError, Result};
use balls::participant::Event;
use balls::participant_config::{override_tokens, InvocationOverrides};
use balls::plugin::{self, Rollback};
use balls::store::task_lock;
use balls::task::{Status, Task, TaskType};
use balls::task_io;

pub fn cmd_update(
    id: String,
    assignments: Vec<String>,
    note: Option<String>,
    identity: Option<String>,
    overrides: InvocationOverrides,
) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    let closing = assignments.iter().any(|a| a == "status=closed");
    // SPEC §7.3: `bl update <id> status=in_progress` is the reject
    // surface. In deferred mode it must also close the auto-opened
    // forge-gate child — see the reject branch below.
    let rejecting = !closing && assignments.iter().any(|a| a == "status=in_progress");
    // Snapshot the state-branch tip before this command's event
    // commit — the rewind target on a required veto (SPEC §9). The
    // tip can't move until `commit_task`/`close_and_archive` below.
    let rb = plugin::state_head(&store)?;
    let (task, task_before) = {
        let _g = task_lock(&store, &id)?;
        let mut task = store.load_task(&id)?;
        let task_before = task.clone();
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
            store.close_and_archive(&task, &msg)?;
        } else if rejecting
            && balls::review_deferred::reject_deferred(
                &store,
                &mut task,
                note.as_deref(),
                &ident,
            )?
        {
            // SPEC §7.3: deferred-mode reject closed the forge-gate
            // child and flipped the parent back in ONE state-branch
            // commit. `reject_deferred` owns the persist; nothing more
            // to write here.
        } else {
            store.save_task(&task)?;
            if let Some(n) = &note {
                task_io::append_note_to(&store.task_path(&id)?, &ident, n)?;
            }
            store.commit_task(&id, &format!("balls: update {} - {}", id, task.title))?;
        }
        (task, task_before)
    };

    let event = if closing { Event::Close } else { Event::Update };
    let tokens = override_tokens(&overrides, false, false);
    plugin::finish(
        &store,
        Some(&task_before),
        &task,
        event,
        &ident,
        &overrides,
        &tokens,
        Rollback::State(rb.as_deref()),
    )?;

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
