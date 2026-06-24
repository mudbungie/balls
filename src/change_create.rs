//! §9 `create` base change — mint a fresh `tasks/<id>.md`. Lifted from [`super`]
//! (the only verb impl with a private helper, [`Create::vanished`]) so the
//! change-verb file stays the occupancy/update/retire trio + the shared message
//! rendering it re-reads through [`super::finalize_titled`].

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use crate::id;
use crate::lifecycle::BaseChange;
use crate::task::{Blocker, On, Task};
use crate::taskfile::{add_blocker, invalid, read_task, task_ids, write_task};
use crate::verb::Verb;

use super::finalize_titled;

/// `create` (§9): mint a fresh `tasks/<id>.md` with no prior state. The id and
/// clock are injected; `existing` is the id set present before the op, so
/// [`BaseChange::finalize`] can find the single new file even after a
/// `create/pre` plugin renamed it.
///
/// The §10/§15 front door splits containment from blocking — NOTHING is
/// auto-minted: `parent` sets the display-only tree pointer and gates nothing;
/// `blockers` are the child's own edges (`--needs B[:OP]`, default `claim`); and
/// `blocks` are reciprocal edges naming the minted `id` on OTHER tasks'
/// ops (`--blocks OP` gates the parent, `--blocks ID:OP` a non-parent — `--gates`
/// is just `--parent X --blocks close`). A `create/pre` id reassignment is the one
/// case `blocks` would not track (the new file is found at finalize, the edge is
/// not).
pub struct Create {
    pub id: String,
    pub actor: String,
    pub now: i64,
    pub title: String,
    pub parent: Option<String>,
    pub priority: Option<i64>,
    pub tags: Vec<String>,
    pub blockers: Vec<Blocker>,
    pub blocks: Vec<(String, On)>,
    /// The `--body` markdown body (§3) — the ball's content, NOT a commit note.
    pub body: Option<String>,
    /// The `-m` free commit-message narration (§5); the subject is the title.
    pub message: Option<String>,
    pub existing: Vec<String>,
}

impl BaseChange for Create {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        let task = Task {
            title: self.title.clone(),
            created: self.now,
            updated: self.now,
            parent: self.parent.clone(),
            priority: self.priority,
            blockers: self.blockers.clone(),
            tags: self.tags.clone(),
            body: self.body.clone().unwrap_or_default(),
            ..Task::default()
        };
        write_task(dir, &self.id, &task)?;
        for (target, on) in &self.blocks {
            add_blocker(dir, target, Blocker { id: self.id.clone(), on: *on }, self.now)?;
        }
        Ok(())
    }

    fn finalize(&self, dir: &Path) -> io::Result<String> {
        let existing: BTreeSet<&str> = self.existing.iter().map(String::as_str).collect();
        let mut new: Vec<String> = task_ids(dir)?
            .into_iter()
            .filter(|id| !existing.contains(id.as_str()))
            .collect();
        let Some(id) = new.pop() else {
            return Err(invalid(self.vanished(dir)));
        };
        if !new.is_empty() {
            return Err(invalid(format!(
                "create: expected exactly one new task file, found {}",
                new.len() + 1
            )));
        }
        if !id::is_valid(&id) {
            return Err(invalid(format!("create: invalid task id '{id}'")));
        }
        finalize_titled(dir, Verb::Create, &self.actor, &id, self.message.as_deref())
    }
}

impl Create {
    /// No new id appeared after `pre`: a `create/pre` reassignment landed ON a
    /// live id (the §id-generation `git mv` seam colliding with an existing
    /// task) or deleted the file outright. Name the collision when the staged
    /// content (this op's clock + title) is found under an existing id —
    /// "expected exactly one new task file, found 0" was oblique (bl-3ddb).
    fn vanished(&self, dir: &Path) -> String {
        let collided = self.existing.iter().find(|id| {
            read_task(dir, id).is_ok_and(|t| t.created == self.now && t.title == self.title)
        });
        match collided {
            Some(id) => format!(
                "create: a create.pre plugin reassigned the new task to `{id}`, which already exists — id collision, nothing sealed"
            ),
            None => "create: expected exactly one new task file, found 0".to_string(),
        }
    }
}
