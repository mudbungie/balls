//! `TaskType` enum. Split from `task.rs` to keep that file under the
//! 300-line cap, mirroring the `link.rs` split pattern.

use crate::error::{BallError, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Type of work item: epic (container), task (default), or bug.
///
/// `Unknown(String)` mirrors `LinkType::Unknown` and `Status::Unknown`:
/// a newer `bl` writing an unfamiliar type must not brick load on older
/// clients. `TaskType::parse` (the CLI entry point) stays strict and
/// never produces `Unknown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskType {
    Epic,
    Task,
    Bug,
    Unknown(String),
}

impl TaskType {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "epic" => Ok(TaskType::Epic),
            "task" => Ok(TaskType::Task),
            "bug" => Ok(TaskType::Bug),
            _ => Err(BallError::InvalidTask(format!("unknown type: {s}"))),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            TaskType::Epic => "epic",
            TaskType::Task => "task",
            TaskType::Bug => "bug",
            TaskType::Unknown(s) => s.as_str(),
        }
    }
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for TaskType {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "epic" => TaskType::Epic,
            "task" => TaskType::Task,
            "bug" => TaskType::Bug,
            _ => TaskType::Unknown(s),
        })
    }
}
