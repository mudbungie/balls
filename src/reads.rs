//! §9 read verbs — `show`, `list`, `dep-tree`. Diffless ops (§8 "skip
//! steps 1/3/5"): they author no ball-file diff and seal nothing; the printed
//! output IS the whole contribution. Each walks `tasks/` on the STORE checkout
//! (§12 — reads run with cwd = the store, never the landing), parses every
//! [`Task`], and renders the §3 derived status ladder plus the §10
//! ready/closeable predicates two ways: a human view (status glyphs, ANSI
//! colour) and `--json` (the supported machine contract).
//!
//! The HUMAN view is the sole place storage's i64 unix-time becomes a date — it
//! renders through [`crate::civil::iso8601`] (§9). `--json` does NOT: it emits
//! the literal stored i64, the lossless export the i64 storage was chosen for
//! (§3). Colour is suppressed for stable ASCII whenever `--plain` is passed,
//! `NO_COLOR` is set, or stdout is not a tty (the last two resolved at the edge
//! — [`Edge::color`]).
//!
//! The silent-empty case (a store with no `tasks/` yet) renders as an empty set,
//! not an error — surfacing an un-primed checkout is the tracker's job (§13),
//! not a read verb's.

use std::collections::HashSet;
use std::io;

use serde_json::{json, Value};

use crate::edge::Edge;
use crate::task::{On, Status, Task};
use crate::taskfile;
use crate::verb::Verb;

mod filter;
mod flags;
mod history;
mod list;
mod show;
mod tree;

pub(crate) use flags::parse;

#[cfg(test)]
pub(crate) mod test_support;

/// Every live ball on the store, parsed once. The id-set is also the §10
/// resolver: "resolved" is file-existence (a closed/dropped ball's file is
/// gone), so a blocker id absent from the catalog is resolved.
pub(crate) struct Catalog {
    entries: Vec<Entry>,
    ids: HashSet<String>,
}

/// One parsed ball: its id (the filename basename, §3) and frontmatter+body.
pub(crate) struct Entry {
    pub id: String,
    pub task: Task,
}

impl Catalog {
    /// Load and parse every `tasks/<id>.md` under the store `dir`. An absent
    /// `tasks/` yields an empty catalog (§13 silent-empty), not an error.
    pub(crate) fn load(dir: &std::path::Path) -> io::Result<Catalog> {
        let mut ids_vec = taskfile::task_ids(dir)?;
        ids_vec.sort();
        let mut entries = Vec::with_capacity(ids_vec.len());
        for id in &ids_vec {
            entries.push(Entry { id: id.clone(), task: taskfile::read_task(dir, id)? });
        }
        let ids = ids_vec.into_iter().collect();
        Ok(Catalog { entries, ids })
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

/// Parsed read-verb flags: the two output toggles shared by every read, plus
/// `list`'s optional §3 status filter and the compose-AND history filters (§9).
#[derive(Default)]
pub(crate) struct Flags {
    pub json: bool,
    pub plain: bool,
    /// `bl list --status ready|blocked|claimed` — the derived ladder (§3) as a
    /// predicate. `None` ⇒ no filter (every live ball). Only `list` accepts it;
    /// it filters the LIVE rung alone (dead balls left no ladder behind).
    pub status: Option<Status>,
    /// `bl list --closed|--all` — how far into history a listing reaches (§9).
    /// `Live` (default) is the current `tasks/`; `Dead`/`All` reconstruct dead
    /// balls most-recent-down. `list`-only.
    pub reach: Reach,
    /// `bl list --tag T` (repeatable): every named tag must be present (AND).
    pub tags: Vec<String>,
    /// `bl list --since DATE`: lower date bound (a day's `00:00:00Z`, §9).
    pub since: Option<i64>,
    /// `bl list --until DATE`: upper date bound, inclusive of the whole day.
    pub until: Option<i64>,
    /// The lone positional: a ball id for `show`, the text-search needle for
    /// `list` (substring over title+body, §9), unused by `dep-tree`.
    pub target: Option<String>,
}

/// How far a `bl list` reaches into the `balls/tasks` history (§9). Dead
/// (closed/dropped) balls are not gone, they are older content (§2) — recovered
/// most-recent-down by the recency walk ([`history`]).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) enum Reach {
    /// Only the live/open set — the current `tasks/*.md` (the default).
    #[default]
    Live,
    /// Only the dead set, reconstructed from history (`--closed`).
    Dead,
    /// Live and dead together (`--all`).
    All,
}

impl Reach {
    /// Does this reach include the live set?
    pub(crate) fn live(self) -> bool {
        matches!(self, Reach::Live | Reach::All)
    }

    /// Does this reach include the dead (history-served) set?
    pub(crate) fn dead(self) -> bool {
        matches!(self, Reach::Dead | Reach::All)
    }
}

/// How a dead ball was retired, derived from its deletion commit's `bl-op:`
/// trailer (§5 — `close` vs `drop`). The one fact history reconstruction adds
/// that the reconstructed frontmatter cannot carry (the file is gone, §2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Retired {
    Closed,
    Dropped,
}

impl Retired {
    /// Read the retirement from a deletion commit's `bl-op` value: `drop` is a
    /// [`Retired::Dropped`], every other op (a `close`, the dominant case) reads
    /// as [`Retired::Closed`] — a balls deletion is only ever one of the two (§5).
    pub(crate) fn from_op(op: &str) -> Retired {
        if op == Verb::Drop.token() {
            Retired::Dropped
        } else {
            Retired::Closed
        }
    }

    /// The stable status word for the retirement — the dead counterpart of
    /// [`status_word`], shown on the human render's badge and `status` line.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Retired::Closed => "closed",
            Retired::Dropped => "dropped",
        }
    }
}

/// The §8 diffless dispatch for the read verbs: load the store catalog, then
/// render. `edge.color` is the resolved host signal (tty AND no `NO_COLOR`);
/// `--plain` overrides it off. Reads never touch the landing or a remote (§13).
pub fn run(edge: &Edge, verb: Verb, args: &[String]) -> io::Result<()> {
    let flags = parse(verb, args)?;
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    let cat = Catalog::load(&store)?;
    let style = Style { plain: flags.plain || !edge.color };
    let out = match verb {
        Verb::Show => show::dispatch(&store, &cat, &flags, &style)?,
        Verb::List => {
            // The dead set is reconstructed from history only when the reach
            // calls for it — the live-only default never touches git (§9).
            let dead = if flags.reach.dead() { history::dead_balls(&store, &cat)? } else { Vec::new() };
            list::render_list(&cat, &dead, &flags, &style)
        }
        Verb::DepTree => tree::render(&cat, &flags, &style),
        other => return Err(io::Error::other(format!("{}: not a read verb", other.token()))),
    };
    print!("{out}");
    Ok(())
}

/// The human-output style: glyphs + ANSI colour, or stable glyph-free ASCII.
pub(crate) struct Style {
    pub plain: bool,
}

impl Style {
    /// The status badge for a line: a coloured glyph in rich mode, a padded
    /// lowercase word in plain mode (`--plain`/`NO_COLOR`/non-tty).
    pub(crate) fn badge(&self, s: Status) -> String {
        if self.plain {
            return format!("{:<8}", status_word(s));
        }
        let (glyph, colour) = match s {
            Status::Ready => ('\u{25cf}', 32),   // ● green
            Status::Claimed => ('\u{25d1}', 36), // ◑ cyan
            Status::Blocked => ('\u{2298}', 33), // ⊘ yellow
        };
        format!("\u{1b}[{colour}m{glyph}\u{1b}[0m")
    }

    /// The badge for a dead (history-served) ball: a dim glyph in rich mode (a
    /// `✓` for a close, a `✗` for a drop), the padded retirement word in plain
    /// mode — the dead counterpart of [`Self::badge`] (§9).
    pub(crate) fn retired_badge(&self, r: Retired) -> String {
        if self.plain {
            return format!("{:<8}", r.word());
        }
        let glyph = match r {
            Retired::Closed => '\u{2713}',  // ✓
            Retired::Dropped => '\u{2717}', // ✗
        };
        format!("\u{1b}[90m{glyph}\u{1b}[0m") // dim grey — retired, not live
    }
}

/// The §3 status word — the stable token shared by plain output and `--json`.
pub(crate) fn status_word(s: Status) -> &'static str {
    match s {
        Status::Ready => "ready",
        Status::Claimed => "claimed",
        Status::Blocked => "blocked",
    }
}

/// The blocker-op token for `--json` and `show`. `on` is ANY op (§10/§15), so it
/// is exactly the verb's canonical token.
pub(crate) fn on_word(on: On) -> &'static str {
    on.token()
}

/// One ball as the **bedrock** JSON record — the single shape every read verb's
/// `--json` emits (§9). It is the lossless mirror of stored frontmatter ONLY:
/// every field round-trips back to the file, and NOTHING derived appears — no
/// `status` ladder, no ISO dates (timestamps stay the literal stored i64), no
/// inverse-derived `children`, no tree nesting. The derived columns live on the
/// orthogonal HUMAN render alone (§3, bl-d074). `id` is the filename identity
/// (the round-trip key), not a frontmatter field.
pub(crate) fn task_json(id: &str, task: &Task) -> Value {
    let blockers: Vec<Value> = task
        .blockers
        .iter()
        .map(|b| json!({ "id": b.id, "on": on_word(b.on) }))
        .collect();
    json!({
        "id": id,
        "title": task.title,
        "claimant": task.claimant,
        "priority": task.priority,
        "parent": task.parent,
        "tags": task.tags,
        "blockers": blockers,
        "created": task.created,
        "updated": task.updated,
    })
}

/// Serialise a JSON value to a trailing-newline string — the one place every
/// `--json` branch renders, so the machine contract is byte-identical.
pub(crate) fn json_line(v: &Value) -> String {
    format!("{}\n", serde_json::to_string_pretty(v).expect("serde_json::Value serializes"))
}

#[cfg(test)]
#[path = "reads_tests.rs"]
mod tests;
