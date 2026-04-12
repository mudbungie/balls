//! Apply plugin sync reports — create, update, and defer local tasks.

use super::basic::generate_unique_id;
use balls::error::Result;
use balls::plugin::SyncReport;
use balls::store::{task_lock, Store};
use balls::task::{NewTaskOpts, Status, Task, TaskType};
use serde_json::Value;

pub fn apply_sync_report(store: &Store, plugin_name: &str, report: &SyncReport) -> Result<()> {
    let cfg = store.load_config()?;

    // Create new tasks from remote
    for item in &report.created {
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

        let id = generate_unique_id(&item.title, store, cfg.id_length)?;
        let mut task = Task::new(opts, id.clone());
        task.status = status;

        let ext_value = Value::Object(item.external.clone());
        task.external.insert(plugin_name.to_string(), ext_value);

        let _g = task_lock(store, &id)?;
        store.save_task(&task)?;
        store.commit_task(&id, &format!("balls: sync-create {} from {}", id, plugin_name))?;
    }

    // Update existing tasks
    for item in &report.updated {
        let _g = task_lock(store, &item.task_id)?;
        let mut task = match store.load_task(&item.task_id) {
            Ok(t) => t,
            Err(_) => {
                eprintln!(
                    "warning: sync update references unknown task {}, skipping",
                    item.task_id
                );
                continue;
            }
        };

        for (field, value) in &item.fields {
            match field.as_str() {
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
                _ => {} // Ignore unknown fields
            }
        }

        if !item.external.is_empty() {
            let ext_value = Value::Object(item.external.clone());
            task.external.insert(plugin_name.to_string(), ext_value);
        }

        if let Some(note) = &item.add_note {
            task.append_note(plugin_name, note);
        }

        task.touch();
        store.save_task(&task)?;
        store.commit_task(
            &item.task_id,
            &format!("balls: sync-update {} from {}", item.task_id, plugin_name),
        )?;
    }

    // Mark deleted tasks as deferred
    for item in &report.deleted {
        let _g = task_lock(store, &item.task_id)?;
        let mut task = match store.load_task(&item.task_id) {
            Ok(t) => t,
            Err(_) => {
                // Task not found — may have been archived. Skip silently.
                continue;
            }
        };

        if task.status == Status::Closed {
            continue;
        }

        task.status = Status::Deferred;
        let reason = if item.reason.is_empty() {
            format!("Deleted in remote tracker ({})", plugin_name)
        } else {
            item.reason.clone()
        };
        task.append_note(plugin_name, &reason);
        task.touch();
        store.save_task(&task)?;
        store.commit_task(
            &item.task_id,
            &format!("balls: sync-defer {} from {}", item.task_id, plugin_name),
        )?;
    }

    Ok(())
}
