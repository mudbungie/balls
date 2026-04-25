//! `ArchivedChild` — the snapshot a parent task carries for each
//! closed descendant. Lives in its own module to keep `task.rs` under
//! the 300-line cap and to localize the forward-compat catch-all.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedChild {
    pub id: String,
    pub title: String,
    pub closed_at: DateTime<Utc>,
    /// Forward-compat passthrough. See `Task::extra` for the rationale.
    /// Closed-children entries are written into the parent task on the
    /// state branch, so a future `bl` adding fields here (e.g. the
    /// delivery sha or a final-status snapshot) must round-trip
    /// through older clients without loss.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}
