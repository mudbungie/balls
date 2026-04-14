//! Apply plugin sync reports — create, update, and defer local tasks.
//!
//! Absorbs per-item failures: a malformed entry logs a warning and moves
//! on rather than aborting the whole report.

use super::basic::generate_unique_id;
use balls::error::Result;
use balls::plugin::SyncReport;
use balls::store::{task_lock, Store};
use balls::task::{NewTaskOpts, Status, Task, TaskType};
use serde_json::Value;

pub fn apply_sync_report(store: &Store, plugin_name: &str, report: &SyncReport) {
    let id_length = store
        .load_config()
        .map(|c| c.id_length)
        .unwrap_or(4);
    for item in &report.created {
        warn_on_err("create", apply_created(store, plugin_name, item, id_length));
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
        eprintln!("warning: sync-report {} failed: {}", what, e);
    }
}

fn apply_created(
    store: &Store,
    plugin_name: &str,
    item: &balls::plugin::SyncCreate,
    id_length: usize,
) -> Result<()> {
    let task_type = TaskType::parse(&item.task_type).unwrap_or(TaskType::Task);
    let priority = item.priority.clamp(1, 4);
    let status = Status::parse(&item.status).unwrap_or(Status::Open);
    let opts = NewTaskOpts {
        title: item.title.clone(),
        task_type,
        priority,
        parent: None,
        depends_on: Vec::new(),
        description: item.description.clone(),
        tags: item.tags.clone(),
    };
    let id = generate_unique_id(&item.title, store, id_length)?;
    let mut task = Task::new(opts, id.clone());
    task.status = status;
    task.external
        .insert(plugin_name.to_string(), Value::Object(item.external.clone()));
    let _g = task_lock(store, &id)?;
    store.save_task(&task)?;
    store.commit_task(&id, &format!("balls: sync-create {} from {}", id, plugin_name))?;
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
    for (field, value) in &item.fields {
        apply_field_update(&mut task, field, value);
    }
    if !item.external.is_empty() {
        task.external
            .insert(plugin_name.to_string(), Value::Object(item.external.clone()));
    }
    task.touch();
    store.save_task(&task)?;
    if let Some(note) = &item.add_note {
        let task_path = store.task_path(&item.task_id)?;
        balls::task_io::append_note_to(&task_path, plugin_name, note)?;
    }
    store.commit_task(
        &item.task_id,
        &format!("balls: sync-update {} from {}", item.task_id, plugin_name),
    )?;
    Ok(())
}

fn apply_field_update(task: &mut Task, field: &str, value: &Value) {
    match field {
        "title" => {
            if let Some(s) = value.as_str() {
                task.title = s.to_string();
            }
        }
        "priority" => {
            if let Some(n) = value.as_u64() {
                task.priority = (n as u8).clamp(1, 4);
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
                task.description = s.to_string();
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
        format!("Deleted in remote tracker ({})", plugin_name)
    } else {
        item.reason.clone()
    };
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
