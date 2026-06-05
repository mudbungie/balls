//! Task-file IO on a worktree dir — the primitives every base change and the
//! gating plugin share, so `tasks/<id>.md`'s path arithmetic and read/write
//! live in ONE place (§3). Pure filesystem ops: no git (the terminus owns
//! that), no clock (the verb layer injects `now`).
//!
//! "Resolved" is file-existence (§10): a closed or dropped ball's file is gone,
//! so [`exists`] is the resolver both `ready`/`closeable` and the gate read.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::task::{Blocker, Task};

/// `<dir>/tasks/<id>.md` — a ball's path within a worktree.
pub(crate) fn task_path(dir: &Path, id: &str) -> PathBuf {
    dir.join("tasks").join(format!("{id}.md"))
}

/// Does `tasks/<id>.md` exist under `dir`? The §10 resolver: a resolved
/// (closed/dropped) ball's file is gone, so absence ⇒ resolved.
pub(crate) fn exists(dir: &Path, id: &str) -> bool {
    task_path(dir, id).exists()
}

/// Read and parse `tasks/<id>.md`; a parse failure maps to invalid-data.
pub(crate) fn read_task(dir: &Path, id: &str) -> io::Result<Task> {
    let text = fs::read_to_string(task_path(dir, id))?;
    Task::parse(&text).map_err(|e| invalid(e.to_string()))
}

/// Render and write `tasks/<id>.md`, creating `tasks/` if absent.
pub(crate) fn write_task(dir: &Path, id: &str, task: &Task) -> io::Result<()> {
    let path = task_path(dir, id);
    fs::create_dir_all(path.parent().expect("task_path always has a tasks/ parent"))?;
    fs::write(path, task.to_markdown())
}

/// Add a `blocker` edge to `tasks/<id>.md` and bump its `updated` to `now`.
/// Idempotent — a duplicate edge is not re-added (§10 front-door reciprocal).
pub(crate) fn add_blocker(dir: &Path, id: &str, blocker: Blocker, now: i64) -> io::Result<()> {
    let mut task = read_task(dir, id)?;
    if !task.blockers.contains(&blocker) {
        task.blockers.push(blocker);
    }
    task.updated = now;
    write_task(dir, id, &task)
}

/// The ids present in `tasks/` (basename minus `.md`); an absent dir is empty.
pub(crate) fn task_ids(dir: &Path) -> io::Result<Vec<String>> {
    let tasks = dir.join("tasks");
    if !tasks.is_dir() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(tasks)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(id) = name.strip_suffix(".md") {
            ids.push(id.to_string());
        }
    }
    Ok(ids)
}

/// An invalid-data error from a String.
pub(crate) fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

#[cfg(test)]
#[path = "taskfile_tests.rs"]
mod tests;
