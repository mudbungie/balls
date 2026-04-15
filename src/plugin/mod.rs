mod diag;
mod limits;
mod runner;
mod types;

pub use runner::Plugin;
pub use types::{PushResponse, SyncCreate, SyncDelete, SyncReport, SyncUpdate};

use crate::config::Config;
use crate::error::Result;
use crate::store::{task_lock, Store};
use crate::task::Task;
use chrono::Utc;
use serde_json::Value;
use std::collections::BTreeMap;

/// Run plugin push for all active plugins. Returns a map of
/// plugin_name -> PushResponse for plugins that returned data.
pub fn run_plugin_push(store: &Store, task: &Task) -> Result<BTreeMap<String, PushResponse>> {
    let cfg = store.load_config()?;
    let mut results = BTreeMap::new();
    for (name, entry) in active_plugins(&cfg) {
        if entry.sync_on_change {
            let plugin = Plugin::resolve(store, name, entry);
            if !plugin.auth_check() {
                continue;
            }
            if let Ok(Some(result)) = plugin.push(task) {
                results.insert(name.clone(), result);
            }
        }
    }
    Ok(results)
}

/// Run plugin sync for all active plugins. Returns (plugin_name, SyncReport) pairs.
pub fn run_plugin_sync(
    store: &Store,
    filter: Option<&str>,
) -> Result<Vec<(String, SyncReport)>> {
    let cfg = store.load_config()?;
    let tasks = store.all_tasks()?;
    let mut reports = Vec::new();
    for (name, entry) in active_plugins(&cfg) {
        let plugin = Plugin::resolve(store, name, entry);
        if !plugin.auth_check() {
            continue;
        }
        if let Ok(Some(report)) = plugin.sync(&tasks, filter) {
            reports.push((name.clone(), report));
        }
    }
    Ok(reports)
}

/// Merge push results into task.external and save+commit.
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

fn active_plugins(cfg: &Config) -> impl Iterator<Item = (&String, &crate::config::PluginEntry)> {
    cfg.plugins.iter().filter(|(_, e)| e.enabled)
}
