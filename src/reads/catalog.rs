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
///
/// This is the public typed read surface a LINKED consumer (yog, DESIGN §16.7
/// U-balls) uses in-process — the exact catalog the `show`/`list` verbs render,
/// the in-process mirror of `bl list --json` / `bl show --json`. It carries NO
/// derived status: a record is stored frontmatter only. Derive status the same
/// way the human render does — [`crate::task::Task::status`] / `ready` /
/// `closeable`, passing [`Catalog::is_resolved`] as the caller-supplied §10
/// resolver. Reads only: mutations stay behind the verb surface (the CAS/plugin
/// protocol is not consumable piecemeal), so no constructor beyond [`load`] is
/// exported. [`load`]: Catalog::load
pub struct Catalog {
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
/// The row a linked consumer reads out of a [`Catalog`] — stored fields only,
/// no derived status (derive it via [`Task::status`] with a resolver).
pub struct Entry {
    pub id: String,
    pub task: Task,
}

impl Catalog {
    /// Load and parse every `tasks/<id>.md` under the store `dir`. An absent
    /// `tasks/` yields an empty catalog (§13 silent-empty), not an error. A
    /// file that fails to parse degrades PER-FILE (bl-528c — corruption can
    /// arrive by hand-edit or merge): it is skipped with a stderr warning
    /// naming it, never failing the whole read.
    ///
    /// The linked-consumer entry point (the in-process `bl list --json`): the
    /// returned catalog is the live set; [`Catalog::entries`] enumerates it and
    /// [`Catalog::get`] resolves one id, exactly as `list` and `show` do.
    pub fn load(dir: &std::path::Path) -> io::Result<Catalog> {
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
    /// closed/dropped ⇒ file gone ⇒ resolved). This is the ready-made §10
    /// resolver a linked consumer hands to [`Task::status`] / `ready` /
    /// `closeable` — `&|id| catalog.is_resolved(id)` — so its own status
    /// derivation matches `bl`'s exactly (corrupt-but-present ids stay
    /// unresolved, as the verbs see them).
    pub fn is_resolved(&self, id: &str) -> bool {
        !self.ids.contains(id)
    }

    /// The §3 derived status of `e`, evaluated against this catalog's resolver.
    pub(crate) fn status(&self, e: &Entry) -> Status {
        e.task.status(&|id| self.is_resolved(id))
    }

    /// Find one live ball by id — the linked-consumer `bl show <id>` on the live
    /// set (a dead ball, resolved from history, is not a catalog member).
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
}
