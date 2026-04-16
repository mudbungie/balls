use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// What a plugin returns on stdout after a successful `push`.
/// Core stores this verbatim into `task.external.{plugin_name}`.
#[derive(Debug, Clone, Deserialize)]
pub struct PushResponse(pub serde_json::Map<String, Value>);

/// Full sync report returned by the plugin on stdout after `sync`.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncReport {
    #[serde(default)]
    pub created: Vec<SyncCreate>,
    #[serde(default)]
    pub updated: Vec<SyncUpdate>,
    #[serde(default)]
    pub deleted: Vec<SyncDelete>,
}

/// A new task to create locally, reported by plugin sync.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncCreate {
    pub title: String,
    #[serde(rename = "type", default = "default_task_type")]
    pub task_type: String,
    #[serde(default = "default_priority")]
    pub priority: u8,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub external: serde_json::Map<String, Value>,
}

/// Fields to update on an existing local task.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncUpdate {
    pub task_id: String,
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    #[serde(default)]
    pub external: serde_json::Map<String, Value>,
    pub add_note: Option<String>,
}

/// A local task to mark as deferred.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncDelete {
    pub task_id: String,
    #[serde(default)]
    pub reason: String,
}

/// A user-facing diagnostic emitted by a plugin on its diagnostics
/// channel (one JSON object per line on the fd referenced by
/// `BALLS_DIAG_FD`). Plugins that ignore the env var never write, and
/// the channel is a silent no-op for them.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginDiagnostic {
    pub level: String,
    pub message: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
}

fn default_task_type() -> String {
    "task".into()
}
fn default_priority() -> u8 {
    3
}
fn default_status() -> String {
    "open".into()
}
