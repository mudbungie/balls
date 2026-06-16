//! §9 deliverable-verb base changes — one [`BaseChange`] per verb.
//!
//! Each verb authors a `tasks/<id>.md` diff at [`BaseChange::stage`], then
//! renders its §5 message at [`BaseChange::finalize`] by RE-READING the
//! post-`pre` tree (so a `create/pre` reassigned id, or a `pre` retitle, is
//! reflected). The impls are git-free — they read/write the worktree dir
//! directly, leaving the [`crate::lifecycle::Engine`]'s anvil the only git in
//! an op — and the clock and minted id are injected, so authoring is pure and
//! unit-testable on a plain temp dir.
//!
//! `claim`/`unclaim`/`close` are NAMED specializations of `update` (§9):
//! [`Occupancy`] fixes `claimant` (claim carries two guards — the already-claimed
//! refusal here, plus the §10 claim-blocker guard via [`crate::enforce`]),
//! [`Retire`] stages the file DELETION, [`Update`] applies a generic
//! [`FieldEdit`] list. Each mutating op runs the SAME op-keyed guard
//! ([`crate::enforce::gate`], §10/§15) for its own verb — `claim`/`close` via
//! their named [`crate::enforce::claim`]/[`crate::enforce::close`] spellings, the
//! rest (`unclaim`/`update`) directly — so a blocker on ANY op is honored.
//! They stay distinct ops because the op NAME is the §6 hook-dispatch key.
//!
//! The §10 guards run at [`BaseChange::stage`] — before the seal, so a refusal
//! aborts the op cleanly, and for `close` before any `close.pre` plugin (e.g.
//! delivery) squashes. The enforcement itself lives in [`crate::enforce`].

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use crate::enforce;
use crate::id;
use crate::lifecycle::BaseChange;
use crate::message::Message;
use crate::task::{Blocker, On, Task};
use crate::taskfile::{add_blocker, invalid, read_task, task_ids, task_path, write_task};
use crate::verb::Verb;

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

/// `claim`/`unclaim` (§9): set or clear the one occupancy field. `claim` carries
/// the one hardcoded guard; `unclaim` is its symmetric clear. "Claimed" is the
/// derived view of `claimant`, so this is the only field either writes.
pub struct Occupancy {
    pub verb: Verb,
    pub id: String,
    pub claimant: Option<String>,
    pub actor: String,
    pub now: i64,
    /// The `-m` free commit-message narration (§5); occupancy edits no ball field.
    pub message: Option<String>,
}

impl Occupancy {
    /// `claim`: take occupancy as `actor` (guarded against an existing claim).
    pub fn claim(id: String, actor: String, now: i64) -> Self {
        Self { verb: Verb::Claim, id, claimant: Some(actor.clone()), actor, now, message: None }
    }

    /// `unclaim`: release occupancy (clear `claimant`).
    pub fn unclaim(id: String, actor: String, now: i64) -> Self {
        Self { verb: Verb::Unclaim, id, claimant: None, actor, now, message: None }
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
            enforce::claim(&task, &self.id, dir)?;
        } else {
            enforce::gate(&task, Verb::Unclaim, &self.id, dir)?;
        }
        task.claimant.clone_from(&self.claimant);
        task.updated = self.now;
        write_task(dir, &self.id, &task)
    }

    fn finalize(&self, dir: &Path) -> io::Result<String> {
        finalize_titled(dir, self.verb, &self.actor, &self.id, self.message.as_deref())
    }
}

/// `update` (§9): the generic field/body edit. Applies an ordered [`FieldEdit`]
/// list and bumps `updated`; an unknown `state:`-style key rides through as a
/// [`FieldEdit::SetExtra`] like any other preserved field (§3). EVERY field is
/// overwriteable here — title, body, parent, priority, tags, extras, blockers —
/// so there is no create-only split; the ball-body edit rides `edits`
/// ([`FieldEdit::Body`]) while `message` is the `-m` commit narration (§5).
pub struct Update {
    pub id: String,
    pub actor: String,
    pub now: i64,
    pub edits: Vec<FieldEdit>,
    /// The `-m` free commit-message narration (§5); the subject is the title.
    pub message: Option<String>,
}

impl BaseChange for Update {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        let mut task = read_task(dir, &self.id)?;
        enforce::gate(&task, Verb::Update, &self.id, dir)?;
        for edit in &self.edits {
            edit.apply(&mut task);
        }
        task.updated = self.now;
        write_task(dir, &self.id, &task)
    }

    fn finalize(&self, dir: &Path) -> io::Result<String> {
        finalize_titled(dir, Verb::Update, &self.actor, &self.id, self.message.as_deref())
    }

    /// `update` is the one base that can stage a byte-identical tree (zero
    /// effective edits + a same-second `updated` restamp), hitting the no-op
    /// seal — its `-m` must refuse to converge rather than drop (bl-cf93).
    fn narrated(&self) -> bool {
        self.message.is_some()
    }
}

// The field-edit vocabulary (one [`FieldEdit`] per overwriteable field, plus
// the `--edit` whole-buffer `Replace`) lives in a sibling module; re-exported
// so consumers keep reaching `crate::change::FieldEdit`.
#[path = "change_field.rs"]
mod field;
pub use field::FieldEdit;

/// `close` (§9): retire a ball — stage the `tasks/<id>.md` DELETION. The
/// `title` is captured before deletion so [`BaseChange::finalize`] can still
/// render a §5 subject once the file is gone. Closing is the ONLY retirement:
/// abandonment is the composite `unclaim` then `close` (the empty deliverable
/// makes the delivery a no-op), so a `--blocks close` gate guards every way a
/// ball can die.
pub struct Retire {
    pub id: String,
    pub title: String,
    pub actor: String,
    /// The `-m` free commit-message narration (§5); retire edits no ball field.
    pub message: Option<String>,
}

impl Retire {
    /// `close`: retire a ball.
    pub fn close(id: String, title: String, actor: String) -> Self {
        Self { id, title, actor, message: None }
    }
}

impl BaseChange for Retire {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        let task = read_task(dir, &self.id)?;
        enforce::close(&task, &self.id, dir)?;
        fs::remove_file(task_path(dir, &self.id))
    }

    fn finalize(&self, _dir: &Path) -> io::Result<String> {
        commit_message(Verb::Close, &self.actor, &self.id, &self.title, self.message.as_deref())
    }
}

/// The shared `finalize` body for the three verbs whose file survives the op
/// (create/claim/unclaim/update): RE-READ the post-`pre` title from `id` and
/// render its §5 message. `close` ([`Retire`]) can't use it — the file is gone,
/// so its title was captured at construction.
fn finalize_titled(dir: &Path, verb: Verb, actor: &str, id: &str, message: Option<&str>) -> io::Result<String> {
    let title = read_task(dir, id)?.title;
    commit_message(verb, actor, id, &title, message)
}

/// Build and render the §5 message: the subject is ALWAYS the ball `title` (the
/// op rides the `bl-op` trailer, not a flavored subject), and `message` is the
/// optional `-m` free body. There is no subject override — the title IS the
/// subject, so `git log` reads naturally and the body is pure narration.
fn commit_message(verb: Verb, actor: &str, id: &str, title: &str, message: Option<&str>) -> io::Result<String> {
    Message {
        verb,
        actor: actor.to_string(),
        id: Some(id.to_string()),
        subject: title.to_string(),
        body: message.map(str::to_string),
    }
    .render()
}

#[cfg(test)]
#[path = "change_tests.rs"]
mod tests;
