//! `TaskType` — a free-form, identifier-safe label attached to each
//! task. Types are pure labels: nothing in balls branches on the
//! value except "is this `epic`?" (which drives progress/display).
//! Widening the vocabulary therefore lives here, not in call sites.
//!
//! The type is a `String` newtype instead of a closed enum so new
//! words like `feature`, `chore`, `spike`, `question` work without a
//! code change. Strict identifier rules are enforced on CLI input
//! (`parse`); deserialization is lenient so an older binary never
//! bricks on a task file written by a newer one.

use crate::error::{BallError, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// One task-type label. Always lowercase; starts with a letter;
/// otherwise letters, digits, `_`, `-`. Filesystem-safe and
/// git-ref-safe by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskType(String);

impl TaskType {
    /// The only type balls treats specially — epic rows get a label
    /// and a progress bar. Named here so call sites avoid a magic
    /// string.
    pub const EPIC: &'static str = "epic";

    /// Common vocabulary surfaced in `-t` help text as suggestions.
    /// These are **not** the only allowed values — any identifier
    /// that passes `is_identifier` works. Kept here (not in config)
    /// so the CLI has something to show without a repo context.
    pub const SUGGESTIONS: &[&str] = &[
        "task",
        "bug",
        "epic",
        "feature",
        "chore",
        "spike",
        "question",
        "discussion",
        "retro",
    ];

    /// Strict constructor for CLI input. Rejects non-identifiers so
    /// `bl create -t 'Fix Things'` fails up front instead of writing
    /// a value that would later confuse greppers and filesystems.
    pub fn parse(s: &str) -> Result<Self> {
        if !is_identifier(s) {
            return Err(BallError::InvalidTask(format!(
                "invalid type: {s:?} (expected identifier like 'task', 'feature', 'spike')"
            )));
        }
        Ok(TaskType(s.to_string()))
    }

    /// Canonical `"task"`. Used as the default when a call site needs
    /// one without going through `parse`.
    pub fn task() -> Self {
        TaskType("task".to_string())
    }

    /// Canonical `"epic"`.
    pub fn epic() -> Self {
        TaskType(Self::EPIC.to_string())
    }

    /// Canonical `"bug"`.
    pub fn bug() -> Self {
        TaskType("bug".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The one behavior-bearing predicate: epics get the `[epic]`
    /// marker and the child-progress bar.
    pub fn is_epic(&self) -> bool {
        self.0 == Self::EPIC
    }
}

/// `^[a-z][a-z0-9_-]*$`, hand-rolled so the crate doesn't pull in
/// `regex` just for this check.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for TaskType {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TaskType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        // Lenient by design: an older `bl` must round-trip a task
        // type written by a newer one even if the value wouldn't
        // pass `parse`. Identifier enforcement is a create-time
        // rule, not a load-time rule.
        let s = String::deserialize(d)?;
        Ok(TaskType(s))
    }
}

#[cfg(test)]
#[path = "task_type_tests.rs"]
mod tests;
