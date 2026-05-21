//! Task-state writes to the `balls/tasks` branch, split out of
//! `store.rs` to keep it under the 300-line cap. These methods are a
//! second `impl Store` block — they route through `Store`'s public
//! accessors only, so no field visibility had to widen for the move.

use crate::error::Result;
use crate::git;
use crate::store::Store;
use crate::store_lock::state_worktree_flock;
use crate::task::{self, Task};
use crate::task_io;
use std::fs;
use std::path::PathBuf;

impl Store {
    /// Stage and commit a task file change on the state branch. No-op
    /// in stealth mode. Stages the sibling notes file too (always
    /// present after a `Task::save`). Holds the store-wide
    /// state-worktree lock for the duration of the git ops.
    pub fn commit_task(&self, id: &str, message: &str) -> Result<()> {
        if self.stealth {
            return Ok(());
        }
        let _g = state_worktree_flock(self)?;
        let dir = self.state_worktree_dir();
        let json = PathBuf::from(format!(".balls/tasks/{id}.json"));
        let notes = PathBuf::from(format!(".balls/tasks/{id}.notes.jsonl"));
        git::git_add(&dir, &[json.as_path(), notes.as_path()])?;
        git::git_commit(&dir, message)?;
        Ok(())
    }

    /// Archive a task and commit the archive on the state branch in a
    /// single locked sequence. Replaces the old `archive_task` +
    /// `commit_staged` pair, which could interleave with another
    /// worker's writes between the `git rm` and the `git commit`.
    ///
    /// The caller has already mutated `task` (status=Closed,
    /// closed_at set, etc.) but must NOT have called `save_task` —
    /// this method handles parent-side bookkeeping and file removal
    /// atomically under the state-worktree lock.
    pub fn close_and_archive(&self, task: &Task, commit_msg: &str) -> Result<()> {
        let mut parent_resaved = false;
        if let Some(pid) = &task.parent {
            if let Ok(mut parent) = self.load_task(pid) {
                parent.closed_children.push(task::ArchivedChild {
                    id: task.id.clone(),
                    title: task.title.clone(),
                    closed_at: task.closed_at.unwrap_or_else(chrono::Utc::now),
                    extra: std::collections::BTreeMap::new(),
                });
                parent.touch();
                self.save_task(&parent)?;
                parent_resaved = true;
            }
        }
        if self.stealth {
            let p = self.task_path(&task.id)?;
            if p.exists() { fs::remove_file(&p)?; }
            task_io::delete_notes_file(&p)?;
            return Ok(());
        }
        let _g = state_worktree_flock(self)?;
        let dir = self.state_worktree_dir();
        if let Some(pid) = task.parent.as_ref().filter(|_| parent_resaved) {
            // Stage the parent's notes sidecar alongside its json: it
            // exists post-`save_task` (mirrors `commit_task`), is a
            // no-op stage when unchanged, and carries the reject note
            // into this same commit on the deferred-reject path. We
            // gate on `parent_resaved` because an already-archived
            // parent leaves no file in the state worktree — staging
            // it would abort close on a missing pathspec.
            let pj = PathBuf::from(format!(".balls/tasks/{pid}.json"));
            let pn = PathBuf::from(format!(".balls/tasks/{pid}.notes.jsonl"));
            git::git_add(&dir, &[pj.as_path(), pn.as_path()])?;
        }
        let json = PathBuf::from(format!(".balls/tasks/{}.json", task.id));
        let notes = PathBuf::from(format!(".balls/tasks/{}.notes.jsonl", task.id));
        git::git_rm_force(&dir, &[json.as_path(), notes.as_path()])?;
        git::git_commit(&dir, commit_msg)?;
        Ok(())
    }

    pub fn all_tasks(&self) -> Result<Vec<Task>> {
        let dir = self.tasks_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            match Task::load(&path) {
                Ok(t) => out.push(t),
                Err(e) => {
                    // Surface malformed but don't abort on one bad file
                    eprintln!("warning: malformed task {}: {}", path.display(), e);
                }
            }
        }
        Ok(out)
    }
}
