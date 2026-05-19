//! Apply plugin sync reports — create, update, and defer local tasks.
//!
//! Absorbs per-item failures: a malformed entry logs a warning and moves
//! on rather than aborting the whole report.

use super::sync_bounds;
use balls::error::Result;
use balls::plugin::SyncReport;
use balls::store::{task_lock, Store};
use balls::task::{NewTaskOpts, Status, Task, TaskType};
use chrono::Utc;
use serde_json::Value;

pub fn apply_sync_report(store: &Store, plugin_name: &str, report: &SyncReport) {
    let (created, dropped) = sync_bounds::clamp_creates(&report.created);
    if dropped > 0 {
        eprintln!(
            "warning: sync-report from {plugin_name}: {dropped} create(s) over the {} flood backstop skipped this sync; raise BALLS_PLUGIN_MAX_SYNC_CREATES if a real store is genuinely this large",
            sync_bounds::max_sync_creates()
        );
    }
    for item in created {
        warn_on_err("create", apply_created(store, plugin_name, item));
    }
    for item in &report.updated {
        warn_on_err(
            &format!("update {}", item.task_id),
            apply_updated(store, plugin_name, item),
        );
    }
    for item in &report.deleted {
        warn_on_err(
            &format!("delete {}", item.task_id),
            apply_deleted(store, plugin_name, item),
        );
    }
}

fn warn_on_err(what: &str, result: Result<()>) {
    if let Err(e) = result {
        eprintln!("warning: sync-report {what} failed: {e}");
    }
}

/// Truncate a synced free-text field in place to the per-field
/// backstop, warning (not failing) if it bit. The whole point is
/// graceful degradation: a pathological multi-hundred-MiB title is
/// clipped with a visible marker and the sync still applies — it is
/// never the reason a real payload's sibling fields get dropped.
fn bound(plugin: &str, what: &str, field: &str, s: &mut String) {
    if let Some(orig) = sync_bounds::truncate_field(s) {
        eprintln!(
            "warning: sync {what} from {plugin}: {field} field was {orig} bytes, truncated to the ingest backstop (raise BALLS_PLUGIN_MAX_SYNC_FIELD_BYTES if this was a real payload)"
        );
    }
}

fn apply_created(
    store: &Store,
    plugin_name: &str,
    item: &balls::plugin::SyncCreate,
) -> Result<()> {
    let task_type = TaskType::parse(&item.task_type).unwrap_or_else(|_| TaskType::task());
    let priority = item.priority.clamp(1, 4);
    let status = Status::parse(&item.status).unwrap_or(Status::Open);
    let mut title = item.title.clone();
    let mut description = item.description.clone();
    bound(plugin_name, "create", "title", &mut title);
    bound(plugin_name, "create", "description", &mut description);
    let opts = NewTaskOpts {
        title: title.clone(),
        task_type,
        priority,
        parent: None,
        depends_on: Vec::new(),
        description,
        tags: item.tags.clone(),
    };
    let id = balls::task_id::generate_task_id(store, &title)?;
    let mut task = Task::new(opts, id.clone());
    task.status = status;
    task.external
        .insert(plugin_name.to_string(), Value::Object(item.external.clone()));
    task.synced_at.insert(plugin_name.to_string(), Utc::now());
    let _g = task_lock(store, &id)?;
    store.save_task(&task)?;
    store.commit_task(&id, &format!("balls: sync-create {id} from {plugin_name}"))?;
    Ok(())
}

fn apply_updated(
    store: &Store,
    plugin_name: &str,
    item: &balls::plugin::SyncUpdate,
) -> Result<()> {
    let _g = task_lock(store, &item.task_id)?;
    let Ok(mut task) = store.load_task(&item.task_id) else {
        eprintln!(
            "warning: sync update references unknown task {}, skipping",
            item.task_id
        );
        return Ok(());
    };
    let what = format!("update {}", item.task_id);
    for (field, value) in &item.fields {
        apply_field_update(plugin_name, &what, &mut task, field, value);
    }
    if !item.external.is_empty() {
        task.external
            .insert(plugin_name.to_string(), Value::Object(item.external.clone()));
    }
    task.synced_at.insert(plugin_name.to_string(), Utc::now());
    task.touch();
    store.save_task(&task)?;
    if let Some(note) = &item.add_note {
        let mut note = note.clone();
        bound(plugin_name, &what, "note", &mut note);
        let task_path = store.task_path(&item.task_id)?;
        balls::task_io::append_note_to(&task_path, plugin_name, &note)?;
    }
    store.commit_task(
        &item.task_id,
        &format!("balls: sync-update {} from {}", item.task_id, plugin_name),
    )?;
    Ok(())
}

fn apply_field_update(plugin: &str, what: &str, task: &mut Task, field: &str, value: &Value) {
    match field {
        "title" => {
            if let Some(s) = value.as_str() {
                let mut t = s.to_string();
                bound(plugin, what, "title", &mut t);
                task.title = t;
            }
        }
        "priority" => {
            if let Some(n) = value.as_u64() {
                task.priority = u8::try_from(n.clamp(1, 4)).unwrap_or(4);
            }
        }
        "status" => {
            if let Some(s) = value.as_str() {
                if let Ok(st) = Status::parse(s) {
                    task.status = st;
                }
            }
        }
        "description" => {
            if let Some(s) = value.as_str() {
                let mut d = s.to_string();
                bound(plugin, what, "description", &mut d);
                task.description = d;
            }
        }
        _ => {}
    }
}

fn apply_deleted(
    store: &Store,
    plugin_name: &str,
    item: &balls::plugin::SyncDelete,
) -> Result<()> {
    let _g = task_lock(store, &item.task_id)?;
    let Ok(mut task) = store.load_task(&item.task_id) else {
        return Ok(());
    };
    if task.status == Status::Closed {
        return Ok(());
    }
    task.status = Status::Deferred;
    let reason = if item.reason.is_empty() {
        format!("Deleted in remote tracker ({plugin_name})")
    } else {
        item.reason.clone()
    };
    task.synced_at.insert(plugin_name.to_string(), Utc::now());
    task.touch();
    store.save_task(&task)?;
    let task_path = store.task_path(&item.task_id)?;
    balls::task_io::append_note_to(&task_path, plugin_name, &reason)?;
    store.commit_task(
        &item.task_id,
        &format!("balls: sync-defer {} from {}", item.task_id, plugin_name),
    )?;
    Ok(())
}
