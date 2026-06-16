//! The store catalog (§9): every live ball parsed once, doubling as the §10
//! blocker resolver (an id absent from the catalog is resolved — its file is
//! gone). Lifted from [`super`] so the read-verb dispatch stays orchestration.

use std::collections::HashSet;
use std::io;

use crate::task::{Status, Task};
use crate::taskfile;

/// Every live ball on the store, parsed once. The id-set is also the §10
/// resolver: "resolved" is file-existence (a closed/dropped ball's file is
/// gone), so a blocker id absent from the catalog is resolved.
pub(crate) struct Catalog {
    /// `pub(super)` so the `list` module's own `impl Catalog` (its `entries()`
    /// accessor + filters) reaches the parsed set; the resolver fields stay private.
    pub(super) entries: Vec<Entry>,
    ids: HashSet<String>,
    /// Balls whose file exists but no longer parses, with each parse error
    /// (bl-528c). One bad ball must not blind the whole store: a corrupt file
    /// is skipped from every listing (warned on stderr at load), but its id
    /// stays in `ids` — the file EXISTS, so a blocker naming it is unresolved —
    /// and `show <id>` surfaces the error instead of "no such ball".
    corrupt: Vec<(String, String)>,
}

/// One parsed ball: its id (the filename basename, §3) and frontmatter+body.
pub(crate) struct Entry {
    pub id: String,
    pub task: Task,
}

impl Catalog {
    /// Load and parse every `tasks/<id>.md` under the store `dir`. An absent
    /// `tasks/` yields an empty catalog (§13 silent-empty), not an error. A
    /// file that fails to parse degrades PER-FILE (bl-528c — corruption can
    /// arrive by hand-edit or merge): it is skipped with a stderr warning
    /// naming it, never failing the whole read.
    pub(crate) fn load(dir: &std::path::Path) -> io::Result<Catalog> {
        let mut ids = taskfile::task_ids(dir)?;
        ids.sort();
        let mut pairs = Vec::with_capacity(ids.len());
        let mut corrupt = Vec::new();
        for id in ids {
            match taskfile::read_task(dir, &id) {
                Ok(task) => pairs.push((id, task)),
                Err(e) => {
                    eprintln!("bl: skipping corrupt ball tasks/{id}.md: {e}");
                    corrupt.push((id, e.to_string()));
                }
            }
        }
        let mut cat = Catalog::from_pairs(pairs);
        cat.ids.extend(corrupt.iter().map(|(id, _)| id.clone()));
        cat.corrupt = corrupt;
        Ok(cat)
    }

    /// A catalog over already-parsed `(id, task)` pairs — the store-free
    /// constructor [`Catalog::load`] reduces to, and the entry point of the §16
    /// `--legacy` projection (whose balls come from a git ref, not `tasks/`).
    pub(crate) fn from_pairs(pairs: Vec<(String, Task)>) -> Catalog {
        let ids = pairs.iter().map(|(id, _)| id.clone()).collect();
        let entries = pairs.into_iter().map(|(id, task)| Entry { id, task }).collect();
        Catalog { entries, ids, corrupt: Vec::new() }
    }

    /// The parse error a corrupt (load-skipped) ball's file carries, by id —
    /// `None` when `id` is not a corrupt file (bl-528c).
    pub(crate) fn corruption(&self, id: &str) -> Option<&str> {
        self.corrupt.iter().find(|(c, _)| c == id).map(|(_, e)| e.as_str())
    }

    /// Is blocker `id` resolved? True when no live ball carries it (§10 —
    /// closed/dropped ⇒ file gone ⇒ resolved).
    pub(crate) fn is_resolved(&self, id: &str) -> bool {
        !self.ids.contains(id)
    }

    /// The §3 derived status of `e`, evaluated against this catalog's resolver.
    pub(crate) fn status(&self, e: &Entry) -> Status {
        e.task.status(&|id| self.is_resolved(id))
    }

    /// Find one ball by id.
    pub(crate) fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
}
