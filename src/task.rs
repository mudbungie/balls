use crate::error::{BallError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Epic,
    Task,
    Bug,
}

impl TaskType {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "epic" => Ok(TaskType::Epic),
            "task" => Ok(TaskType::Task),
            "bug" => Ok(TaskType::Bug),
            _ => Err(BallError::InvalidTask(format!("unknown type: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Open,
    InProgress,
    Blocked,
    Closed,
    Deferred,
}

impl Status {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "open" => Ok(Status::Open),
            "in_progress" => Ok(Status::InProgress),
            "blocked" => Ok(Status::Blocked),
            "closed" => Ok(Status::Closed),
            "deferred" => Ok(Status::Deferred),
            _ => Err(BallError::InvalidTask(format!("unknown status: {}", s))),
        }
    }

    pub fn precedence(&self) -> u8 {
        match self {
            Status::Closed => 5,
            Status::InProgress => 4,
            Status::Blocked => 3,
            Status::Open => 2,
            Status::Deferred => 1,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Blocked => "blocked",
            Status::Closed => "closed",
            Status::Deferred => "deferred",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub ts: DateTime<Utc>,
    pub author: String,
    pub text: String,
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
    pub closed_children: Vec<ArchivedChild>,
    #[serde(default)]
    pub external: BTreeMap<String, Value>,
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
            closed_children: Vec::new(),
            external: BTreeMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path)?;
        let t: Task = serde_json::from_str(&s)
            .map_err(|e| BallError::InvalidTask(format!("{}: {}", path.display(), e)))?;
        Ok(t)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Atomic write via tmp + rename
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, s + "\n")?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn append_note(&mut self, author: &str, text: &str) {
        self.notes.push(Note {
            ts: Utc::now(),
            author: author.to_string(),
            text: text.to_string(),
        });
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
