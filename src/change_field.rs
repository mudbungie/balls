//! The §9 `update` edit vocabulary — one [`FieldEdit`] per overwriteable ball
//! field, applied in order by [`super::Update`]. Split from [`crate::change`]
//! (the per-verb base changes) so the field-edit meaning lives in one sibling
//! place; `change` re-exports it, so consumers still reach
//! `crate::change::FieldEdit`.

use crate::task::{Blocker, Task};

/// One field-level mutation [`super::Update`] applies. `Parent`/`Priority` carry
/// an `Option` so the same variant sets or clears; tags and blockers add/remove
/// idempotently; `Extra` reaches a team's own preserved key; [`Replace`] is the
/// `--edit` whole-buffer overwrite.
///
/// [`Replace`]: FieldEdit::Replace
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
    /// The `--edit` whole-buffer overwrite: an $EDITOR-validated [`Task`]
    /// replaces every stored field. `created` is preserved from the stored ball
    /// (the id is the path and `created` is history — neither is hand-editable)
    /// and `updated` is restamped by the seal, so a hand-typed value never
    /// survives.
    Replace(Box<Task>),
}

impl FieldEdit {
    pub(super) fn apply(&self, task: &mut Task) {
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
            FieldEdit::Replace(after) => {
                let created = task.created;
                *task = (**after).clone();
                task.created = created;
            }
        }
    }
}
