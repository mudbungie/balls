//! Task file I/O. Separated from `task.rs` to keep that file under the
//! 300-line cap and to localize the on-disk format in one place.
//!
//! Format rules (see docs/SPEC-orphan-branch-state.md §5):
//! - Top-level JSON keys sorted alphabetically.
//! - One key per line, value serialized compact on the same line.
//! - Trailing newline.
//! - Notes are NOT stored inside the task file; they live in an
//!   append-only sibling `<id>.notes.jsonl`. This keeps concurrent
//!   note appends from two workers text-mergeable.

use crate::error::{BallError, Result};
use crate::task::{validate_id, Note, Task};
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `.../bl-abc.json` -> `.../bl-abc.notes.jsonl`.
pub(crate) fn notes_path_for(task_path: &Path) -> PathBuf {
    let stem = task_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("task");
    let parent = task_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}.notes.jsonl"))
}

fn load_notes_file(path: &Path) -> Result<Vec<Note>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let s = fs::read_to_string(path)?;
    let mut notes = Vec::new();
    for line in s.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let n: Note = serde_json::from_str(line)
            .map_err(|e| BallError::InvalidTask(format!("{}: {}", path.display(), e)))?;
        notes.push(n);
    }
    Ok(notes)
}

/// Append a single note to the sibling notes file for a task. Uses
/// `O_APPEND` so two concurrent writers each add exactly one line without
/// racing on a read-modify-write cycle. Two lines at the end of a file
/// merge cleanly under stock `git merge` in the common case.
pub fn append_note_to(task_path: &Path, author: &str, text: &str) -> Result<Note> {
    let note = Note {
        ts: Utc::now(),
        author: author.to_string(),
        text: text.to_string(),
        extra: std::collections::BTreeMap::new(),
    };
    let notes_path = notes_path_for(task_path);
    if let Some(parent) = notes_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(&note)?;
    line.push('\n');
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&notes_path)?;
    f.write_all(line.as_bytes())?;
    Ok(note)
}

/// Delete the sibling notes file if present. Used at task close / drop.
pub(crate) fn delete_notes_file(task_path: &Path) -> Result<()> {
    let notes_path = notes_path_for(task_path);
    if notes_path.exists() {
        fs::remove_file(&notes_path)?;
    }
    Ok(())
}

impl Task {
    /// Load a task from `path`. Notes live in a sibling file; load handles
    /// both the new layout (notes in sibling) and legacy task files whose
    /// `notes` array is embedded in the JSON. Either way, the returned
    /// `Task.notes` is the union of both sources.
    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path)?;
        let mut t: Task = serde_json::from_str(&s)
            .map_err(|e| BallError::InvalidTask(format!("{}: {}", path.display(), e)))?;
        validate_id(&t.id)?;
        let sidecar = load_notes_file(&notes_path_for(path))?;
        t.notes.extend(sidecar);
        Ok(t)
    }

    /// Persist a task to `path` in the text-mergeable format. Never writes
    /// the notes sidecar content; use `append_note_to` for that. Does
    /// ensure the notes sidecar exists as an empty file on first save, so
    /// that concurrent first-append operations on divergent branches see
    /// the file in the merge base and use git's union merge driver
    /// instead of racing on add/add.
    pub fn save(&self, path: &Path) -> Result<()> {
        let body = serialize_mergeable(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, body.as_bytes())?;
        fs::rename(&tmp, path)?;

        let notes_path = notes_path_for(path);
        if !notes_path.exists() {
            fs::write(&notes_path, "")?;
        }
        Ok(())
    }
}

/// Serialize a task as a sorted-key, one-field-per-line JSON object with
/// compact single-line values and a trailing newline. The `notes` field is
/// excluded; notes live in the sibling file.
pub(crate) fn serialize_mergeable(task: &Task) -> Result<String> {
    let v = serde_json::to_value(task)?;
    let obj = v
        .as_object()
        .ok_or_else(|| BallError::InvalidTask("task did not serialize to an object".into()))?;

    let mut keys: Vec<&String> = obj.keys().filter(|k| *k != "notes").collect();
    keys.sort();

    let mut s = String::from("{\n");
    for (i, k) in keys.iter().enumerate() {
        let key_json = serde_json::to_string(k)?;
        let val_json = serde_json::to_string(&obj[*k])?;
        s.push_str("  ");
        s.push_str(&key_json);
        s.push_str(": ");
        s.push_str(&val_json);
        if i + 1 < keys.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("}\n");
    Ok(s)
}

#[cfg(test)]
#[path = "task_io_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "task_io_compat_tests.rs"]
mod compat_tests;
