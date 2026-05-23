use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

// `Status`, `LinkType`/`Link`, `TaskType`, and `ArchivedChild` live
// in their own modules to keep this file under the 300-line cap and
// to localize each self-contained type. Re-exported here for call
// sites that import from `balls::task`.
pub use crate::archived_child::ArchivedChild;
pub use crate::link::{Link, LinkType};
pub use crate::status::Status;
pub use crate::task_type::TaskType;
pub use crate::task_validate::{
    parse_priority, validate_id, validate_priority, PRIORITY_MAX, PRIORITY_MIN,
};

/// Append-only note attached to a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub ts: DateTime<Utc>,
    pub author: String,
    pub text: String,
    /// Forward-compat passthrough; see the `Task::extra` doc for the
    /// full rationale.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub priority: u8,
    pub status: Status,
    pub parent: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub claimed_by: Option<String>,
    pub branch: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub closed_children: Vec<ArchivedChild>,
    #[serde(default)]
    pub external: BTreeMap<String, Value>,
    /// Per-plugin timestamp of the last applied push/sync response.
    /// Plugins compare their remote's `updated_at` against this for
    /// bidirectional conflict resolution without a side-cache. Missing
    /// keys mean "never synced by that plugin".
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub synced_at: BTreeMap<String, DateTime<Utc>>,
    /// SPEC §9/§8.1 — per-participant verbatim reason a native
    /// best-effort negotiation did not land (a `reject` reason or
    /// wire-failure message). Set on skip, cleared on next success.
    /// Legacy-shim skips stay silent so SPEC §12 byte-identity holds.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sync_status: BTreeMap<String, String>,
    /// Performance hint: SHA of the squash-merge on main that
    /// delivered this task. Ground truth is the `[id]` tag embedded
    /// in the commit message — see `crate::delivery`.
    #[serde(default)]
    pub delivered_in: Option<String>,
    /// Code-home provenance: the code repo this task's work belongs
    /// to, as a fetchable `origin` URL. `bl create` stamps the
    /// creating clone's origin; `bl claim` re-anchors it to the
    /// claiming clone — definitionally the code home (bl-8994). Only
    /// a real URL is auto-written, never a bare basename, so a null
    /// means "origin unknown," not "single-repo." Implicitly frozen
    /// after claim: no lifecycle step re-stamps it, though an explicit
    /// `bl update <id> repo=` still can. Older `bl` round-trips it
    /// via `extra` (SPEC §13 / bl-d31c).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Delivery provenance (bl-7523): the code repo whose history
    /// contains `delivered_in`. Set wherever `delivered_in` is set —
    /// `bl review` (local-squash) and `bl close` (deferred / manual
    /// `--delivered`). Distinct from `repo` because a task may be
    /// created in client A and delivered from client B once tasks
    /// live against a shared tracker (bl-ffb4). Missing on tasks delivered by
    /// a pre-bl-7523 `bl` — readers should interpret a null as "the
    /// locally-checked-out repo," which keeps single-repo
    /// deployments byte-identical. Older `bl` round-trips it via
    /// `extra` (SPEC §13 / bl-d31c).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_repo: Option<String>,
    /// Per-task override of the repo-level `config.target_branch`
    /// (SPEC §6.2). When set, `bl review` squashes this task into
    /// this branch, ignoring both the repo default and the
    /// current-branch fallback — the smallest unit that expresses
    /// git-flow's hotfix→main vs feature→develop split. Optional and
    /// `skip_serializing_if` None, so existing task files stay
    /// byte-identical and an older `bl` (no `deny_unknown_fields`)
    /// silently round-trips it via `extra`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
    /// Forward-compat passthrough. Any top-level JSON field that the
    /// current struct doesn't name lands here on deserialize and
    /// round-trips back out on save. Lets an older `bl` load a task
    /// file written by a future version without silently dropping
    /// new first-party fields. `external` already exists for plugin
    /// data; `extra` is the catch-all for everything else.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

pub struct NewTaskOpts {
    pub title: String,
    pub task_type: TaskType,
    pub priority: u8,
    pub parent: Option<String>,
    pub depends_on: Vec<String>,
    pub description: String,
    pub tags: Vec<String>,
}

impl Default for NewTaskOpts {
    fn default() -> Self {
        NewTaskOpts {
            title: String::new(),
            task_type: TaskType::task(),
            priority: 3,
            parent: None,
            depends_on: Vec::new(),
            description: String::new(),
            tags: Vec::new(),
        }
    }
}

impl Task {
    /// Task ids are persisted forever once minted, and id generation
    /// must work in stealth/no-git mode. SHA-1 here is load-bearing
    /// on-disk state, same shape as `store_paths::stealth_tasks_dir`:
    /// the algorithm and its byte output are the on-disk contract, not
    /// an implementation detail. Backed by the vendored in-tree
    /// `crate::hash::sha1_hex` so the RustCrypto stack is not in the
    /// dependency tree.
    pub fn generate_id(title: &str, ts: DateTime<Utc>, id_length: usize) -> String {
        let ts_str = ts.to_rfc3339();
        let mut buf = Vec::with_capacity(title.len() + ts_str.len());
        buf.extend_from_slice(title.as_bytes());
        buf.extend_from_slice(ts_str.as_bytes());
        let hex = crate::hash::sha1_hex(&buf);
        format!("bl-{}", &hex[..id_length])
    }

    pub fn new(opts: NewTaskOpts, id: String) -> Self {
        let now = Utc::now();
        Task {
            id,
            title: opts.title,
            task_type: opts.task_type,
            priority: opts.priority,
            status: Status::Open,
            parent: opts.parent,
            depends_on: opts.depends_on,
            description: opts.description,
            created_at: now,
            updated_at: now,
            closed_at: None,
            claimed_by: None,
            branch: None,
            tags: opts.tags,
            notes: Vec::new(),
            links: Vec::new(),
            closed_children: Vec::new(),
            external: BTreeMap::new(),
            synced_at: BTreeMap::new(),
            sync_status: BTreeMap::new(),
            delivered_in: None,
            repo: None,
            delivered_repo: None,
            target_branch: None,
            extra: BTreeMap::new(),
        }
    }

    // `save` and `load` live in `task_io.rs` to keep this file focused on
    // type definitions and to localize the on-disk format in one module.

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// In-memory note append. **Does not persist.** For on-disk persistence,
    /// use `task_io::append_note_to`, which writes to the append-only
    /// sibling file and is safe for concurrent writers.
    pub fn append_note(&mut self, author: &str, text: &str) {
        self.notes.push(Note {
            ts: Utc::now(),
            author: author.to_string(),
            text: text.to_string(),
            extra: BTreeMap::new(),
        });
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
