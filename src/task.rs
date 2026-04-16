use crate::error::{BallError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fmt;

// `LinkType`/`Link` and `TaskType` live in their own modules to keep
// this file under the 300-line cap. Re-exported here for call sites
// that import from `balls::task`.
pub use crate::link::{Link, LinkType};
pub use crate::task_type::TaskType;

/// Task lifecycle status.
///
/// `Unknown(String)` exists purely for forward compatibility, mirroring
/// `LinkType::Unknown`: if a newer `bl` writes a status we don't recognize,
/// older clients round-trip it verbatim instead of hard-erroring on the
/// whole task file. `Status::parse` (the CLI entry point) never produces
/// `Unknown` — users cannot craft one by hand. An `Unknown` status has the
/// lowest precedence, so conflict resolution never accidentally elects it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Open,
    InProgress,
    Review,
    Blocked,
    Closed,
    Deferred,
    Unknown(String),
}

impl Status {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "open" => Ok(Status::Open),
            "in_progress" => Ok(Status::InProgress),
            "review" => Ok(Status::Review),
            "blocked" => Ok(Status::Blocked),
            "closed" => Ok(Status::Closed),
            "deferred" => Ok(Status::Deferred),
            _ => Err(BallError::InvalidTask(format!("unknown status: {s}"))),
        }
    }

    pub fn precedence(&self) -> u8 {
        match self {
            Status::Closed => 6,
            Status::Review => 5,
            Status::InProgress => 4,
            Status::Blocked => 3,
            Status::Open => 2,
            Status::Deferred => 1,
            Status::Unknown(_) => 0,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Review => "review",
            Status::Blocked => "blocked",
            Status::Closed => "closed",
            Status::Deferred => "deferred",
            Status::Unknown(s) => s.as_str(),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "open" => Status::Open,
            "in_progress" => Status::InProgress,
            "review" => Status::Review,
            "blocked" => Status::Blocked,
            "closed" => Status::Closed,
            "deferred" => Status::Deferred,
            _ => Status::Unknown(s),
        })
    }
}

/// Append-only note attached to a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub ts: DateTime<Utc>,
    pub author: String,
    pub text: String,
    /// Forward-compat passthrough: unknown fields from a newer `bl`
    /// land here on deserialize and round-trip back out on save. See
    /// the `Task::extra` doc for the full rationale.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedChild {
    pub id: String,
    pub title: String,
    pub closed_at: DateTime<Utc>,
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
    /// Per-plugin timestamp of the last time balls applied a push or
    /// sync response for this task from the named plugin. Plugins
    /// compare their remote's `updated_at` against this value for
    /// bidirectional conflict resolution without maintaining a
    /// side-cache. Written by balls, sent back on every push/sync.
    /// Missing keys mean "never synced by that plugin".
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub synced_at: BTreeMap<String, DateTime<Utc>>,
    /// Performance hint: SHA of the squash-merge on main that
    /// delivered this task. Ground truth is the `[id]` tag embedded
    /// in the commit message — see `crate::delivery`.
    #[serde(default)]
    pub delivered_in: Option<String>,
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
            task_type: TaskType::Task,
            priority: 3,
            parent: None,
            depends_on: Vec::new(),
            description: String::new(),
            tags: Vec::new(),
        }
    }
}

/// Inclusive range for task priority. `1` is highest urgency, `4`
/// lowest. Centralized here so a future widening (e.g. to `1..=5`)
/// touches exactly one place.
pub const PRIORITY_MIN: u8 = 1;
pub const PRIORITY_MAX: u8 = 4;

/// Reject priorities outside `PRIORITY_MIN..=PRIORITY_MAX`. Used by
/// the `bl create` path, which already receives a `u8` from clap.
pub fn validate_priority(p: u8) -> Result<()> {
    if !(PRIORITY_MIN..=PRIORITY_MAX).contains(&p) {
        return Err(BallError::InvalidTask(format!(
            "priority must be {PRIORITY_MIN}..={PRIORITY_MAX}"
        )));
    }
    Ok(())
}

/// Parse a priority from a user-supplied string (e.g. the `value`
/// half of `bl update priority=3`). Rejects non-integers and
/// out-of-range values with a consistent error message.
pub fn parse_priority(s: &str) -> Result<u8> {
    let p: u8 = s
        .parse()
        .map_err(|_| BallError::InvalidTask(format!("priority not integer: {s}")))?;
    validate_priority(p)?;
    Ok(p)
}

/// Validate that a task ID is safe for use in file paths.
///
/// IDs must match `bl-[0-9a-fA-F]+`. `generate_id` only ever emits
/// lowercase hex, but the loader accepts uppercase or mixed case so
/// a future `bl` that changes its generator does not break older
/// clients reading the same repo. The `bl-` prefix itself is
/// deliberately strict: mixing `bl-` and `BL-` in one repo would
/// fragment task filenames, so we surface that as a hard error.
pub fn validate_id(id: &str) -> Result<()> {
    let valid = id.starts_with("bl-")
        && id.len() > 3
        && id[3..].chars().all(|c| c.is_ascii_hexdigit());
    if !valid {
        return Err(BallError::InvalidTask(format!("invalid task id: {id}")));
    }
    Ok(())
}

impl Task {
    pub fn generate_id(title: &str, ts: DateTime<Utc>, id_length: usize) -> String {
        let mut hasher = Sha1::new();
        hasher.update(title.as_bytes());
        hasher.update(ts.to_rfc3339().as_bytes());
        let digest = hasher.finalize();
        let hex = hex::encode(digest);
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
            delivered_in: None,
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
