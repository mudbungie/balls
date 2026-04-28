mod diag;
mod dispatch;
mod limits;
mod participant;
mod runner;
mod types;

pub use dispatch::{dispatch_push, dispatch_sync};
pub use participant::{LegacyOutcome, LegacyPluginParticipant};
pub use runner::Plugin;
pub use types::{PushResponse, SyncCreate, SyncDelete, SyncReport, SyncUpdate};

use crate::error::Result;
use crate::store::{task_lock, Store};
use chrono::Utc;
use serde_json::Value;
use std::collections::BTreeMap;

/// Merge push results into task.external and save+commit. Used by the
/// dispatcher to apply the aggregated outcome of one lifecycle event
/// in a single state-branch commit.
pub fn apply_push_response(
    store: &Store,
    task_id: &str,
    results: &BTreeMap<String, PushResponse>,
) -> Result<()> {
    if results.is_empty() {
        return Ok(());
    }
    let _g = task_lock(store, task_id)?;
    let mut task = store.load_task(task_id)?;
    let now = Utc::now();
    for (plugin_name, response) in results {
        let ext_value = Value::Object(response.0.clone());
        task.external.insert(plugin_name.clone(), ext_value);
        task.synced_at.insert(plugin_name.clone(), now);
    }
    task.touch();
    store.save_task(&task)?;
    store.commit_task(task_id, &format!("balls: update external for {task_id}"))?;
    Ok(())
}
