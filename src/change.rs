//! §9 deliverable-verb base changes — one [`BaseChange`] per verb.
//!
//! Each verb authors a `tasks/<id>.md` diff at [`BaseChange::stage`], then
//! renders its §5 message at [`BaseChange::finalize`] by RE-READING the
//! post-`pre` tree (so a `create/pre` reassigned id, or a `pre` retitle, is
//! reflected). The impls are git-free — they read/write the worktree dir
//! directly, leaving the [`crate::lifecycle::Engine`]'s terminus the only git in
//! an op — and the clock and minted id are injected, so authoring is pure and
//! unit-testable on a plain temp dir.
//!
//! `claim`/`unclaim`/`close`/`drop` are NAMED specializations of `update` (§9):
//! [`Occupancy`] fixes `claimant` (claim carries the one hardcoded guard —
//! refuse a ball whose `claimant` is already set), [`Retire`] stages the file
//! DELETION, [`Update`] applies a generic [`FieldEdit`] list. They stay distinct
//! ops because the op NAME is the §6 hook-dispatch key.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use crate::id;
use crate::lifecycle::BaseChange;
use crate::message::{self, Message};
use crate::task::{Blocker, On, Task};
use crate::taskfile::{add_blocker, invalid, read_task, task_ids, task_path, write_task};
use crate::verb::Verb;

/// `create` (§9): mint a fresh `tasks/<id>.md` with no prior state. The id and
/// clock are injected; `existing` is the id set present before the op, so
/// [`BaseChange::finalize`] can find the single new file even after a
/// `create/pre` plugin renamed it.
///
/// `parent` carries the §10 front-door semantics of `--parent`/`--gates`: both
/// set the child's `parent`, differing only in the reciprocal blocker `on` we
/// add to that parent — `On::Claim` (subtask claim-blocks its epic) for
/// `--parent`, `On::Close` (gate close-blocks its parent) for `--gates`.
/// `--needs` is just a claim-blocker on the child itself (`blockers`). The
/// reciprocal names the minted `id`; a `create/pre` id reassignment is the one
/// case it would not track (the new file is found at finalize, the edge is not).
pub struct Create {
    pub id: String,
    pub actor: String,
    pub now: i64,
    pub title: String,
    pub parent: Option<(String, On)>,
    pub priority: Option<i64>,
    pub tags: Vec<String>,
    pub blockers: Vec<Blocker>,
    pub over: Option<String>,
    pub body: Option<String>,
    pub existing: Vec<String>,
}

impl BaseChange for Create {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        let task = Task {
            title: self.title.clone(),
            created: self.now,
            updated: self.now,
            parent: self.parent.as_ref().map(|(id, _)| id.clone()),
            priority: self.priority,
            blockers: self.blockers.clone(),
            tags: self.tags.clone(),
            ..Task::default()
        };
        write_task(dir, &self.id, &task)?;
        if let Some((parent_id, on)) = &self.parent {
            add_blocker(dir, parent_id, Blocker { id: self.id.clone(), on: *on }, self.now)?;
        }
        Ok(())
    }

    fn finalize(&self, dir: &Path) -> io::Result<String> {
        let existing: BTreeSet<&str> = self.existing.iter().map(String::as_str).collect();
        let mut new: Vec<String> = task_ids(dir)?
            .into_iter()
            .filter(|id| !existing.contains(id.as_str()))
            .collect();
        if new.len() != 1 {
            return Err(invalid(format!(
                "create: expected exactly one new task file, found {}",
                new.len()
            )));
        }
        let id = new.pop().expect("len checked == 1");
        if !id::is_valid(&id) {
            return Err(invalid(format!("create: invalid task id '{id}'")));
        }
        let title = read_task(dir, &id)?.title;
        commit_message(Verb::Create, &self.actor, &id, self.over.as_deref(), &title, self.body.as_deref())
    }
}

/// `claim`/`unclaim` (§9): set or clear the one occupancy field. `claim` carries
/// the one hardcoded guard; `unclaim` is its symmetric clear. "Claimed" is the
/// derived view of `claimant`, so this is the only field either writes.
pub struct Occupancy {
    pub verb: Verb,
    pub id: String,
    pub claimant: Option<String>,
    pub actor: String,
    pub now: i64,
    pub over: Option<String>,
    pub body: Option<String>,
}

impl Occupancy {
    /// `claim`: take occupancy as `actor` (guarded against an existing claim).
    pub fn claim(id: String, actor: String, now: i64) -> Self {
        Self { verb: Verb::Claim, id, claimant: Some(actor.clone()), actor, now, over: None, body: None }
    }

    /// `unclaim`: release occupancy (clear `claimant`).
    pub fn unclaim(id: String, actor: String, now: i64) -> Self {
        Self { verb: Verb::Unclaim, id, claimant: None, actor, now, over: None, body: None }
    }
}

impl BaseChange for Occupancy {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        let mut task = read_task(dir, &self.id)?;
        if self.verb == Verb::Claim {
            if let Some(who) = &task.claimant {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("claim: {} is already claimed by {who}", self.id),
                ));
            }
        }
        task.claimant.clone_from(&self.claimant);
        task.updated = self.now;
        write_task(dir, &self.id, &task)
    }

    fn finalize(&self, dir: &Path) -> io::Result<String> {
        let title = read_task(dir, &self.id)?.title;
        commit_message(self.verb, &self.actor, &self.id, self.over.as_deref(), &title, self.body.as_deref())
    }
}

/// `update` (§9): the generic field/body edit. Applies an ordered [`FieldEdit`]
/// list and bumps `updated`; an unknown `state:`-style key rides through as a
/// [`FieldEdit::SetExtra`] like any other preserved field (§3).
pub struct Update {
    pub id: String,
    pub actor: String,
    pub now: i64,
    pub edits: Vec<FieldEdit>,
    pub over: Option<String>,
    pub body: Option<String>,
}

impl BaseChange for Update {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        let mut task = read_task(dir, &self.id)?;
        for edit in &self.edits {
            edit.apply(&mut task);
        }
        task.updated = self.now;
        write_task(dir, &self.id, &task)
    }

    fn finalize(&self, dir: &Path) -> io::Result<String> {
        let title = read_task(dir, &self.id)?.title;
        commit_message(Verb::Update, &self.actor, &self.id, self.over.as_deref(), &title, self.body.as_deref())
    }
}

/// One field-level mutation [`Update`] applies. `Parent`/`Priority` carry an
/// `Option` so the same variant sets or clears; tags and blockers add/remove
/// idempotently; `Extra` reaches a team's own preserved key.
pub enum FieldEdit {
    Title(String),
    Body(String),
    Parent(Option<String>),
    Priority(Option<i64>),
    AddTag(String),
    RemoveTag(String),
    AddBlocker(Blocker),
    RemoveBlocker(String),
    SetExtra(String, toml::Value),
    RemoveExtra(String),
}

impl FieldEdit {
    fn apply(&self, task: &mut Task) {
        match self {
            FieldEdit::Title(t) => task.title.clone_from(t),
            FieldEdit::Body(b) => task.body.clone_from(b),
            FieldEdit::Parent(p) => task.parent.clone_from(p),
            FieldEdit::Priority(p) => task.priority = *p,
            FieldEdit::AddTag(t) => {
                if !task.tags.contains(t) {
                    task.tags.push(t.clone());
                }
            }
            FieldEdit::RemoveTag(t) => task.tags.retain(|x| x != t),
            FieldEdit::AddBlocker(b) => {
                if !task.blockers.contains(b) {
                    task.blockers.push(b.clone());
                }
            }
            FieldEdit::RemoveBlocker(id) => task.blockers.retain(|b| b.id != *id),
            FieldEdit::SetExtra(k, v) => {
                task.extra.insert(k.clone(), v.clone());
            }
            FieldEdit::RemoveExtra(k) => {
                task.extra.remove(k.as_str());
            }
        }
    }
}

/// `close`/`drop` (§9): retire a ball — both stage the `tasks/<id>.md` DELETION;
/// the only difference is intent, carried by the `bl-op` trailer (`verb`). The
/// `title` is captured before deletion so [`BaseChange::finalize`] can still
/// render a §5 subject once the file is gone.
pub struct Retire {
    pub verb: Verb,
    pub id: String,
    pub title: String,
    pub actor: String,
    pub over: Option<String>,
    pub body: Option<String>,
}

impl Retire {
    /// `close`: retire a delivered ball.
    pub fn close(id: String, title: String, actor: String) -> Self {
        Self { verb: Verb::Close, id, title, actor, over: None, body: None }
    }

    /// `drop`: abandon a ball.
    pub fn drop(id: String, title: String, actor: String) -> Self {
        Self { verb: Verb::Drop, id, title, actor, over: None, body: None }
    }
}

impl BaseChange for Retire {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        fs::remove_file(task_path(dir, &self.id))
    }

    fn finalize(&self, _dir: &Path) -> io::Result<String> {
        commit_message(self.verb, &self.actor, &self.id, self.over.as_deref(), &self.title, self.body.as_deref())
    }
}

/// Build and render the §5 message: subject is the `-m` override else `title`.
fn commit_message(
    verb: Verb,
    actor: &str,
    id: &str,
    over: Option<&str>,
    title: &str,
    body: Option<&str>,
) -> io::Result<String> {
    Message {
        verb,
        actor: actor.to_string(),
        id: Some(id.to_string()),
        subject: message::subject(over, title),
        body: body.map(str::to_string),
    }
    .render()
}

#[cfg(test)]
#[path = "change_tests.rs"]
mod tests;
