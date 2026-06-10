//! §9 read verbs — `show`, `list`, `dep-tree`. Diffless ops (§8 "skip
//! steps 1/3/5"): they author no ball-file diff and seal nothing; the printed
//! output IS the whole contribution. The human render of `show` may also FOLD
//! IN a §6 read-op plugin dispatch ([`readop`] — the delivery worktree line,
//! §11); `--json` never dispatches. Each walks `tasks/` on the STORE checkout
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
mod readop;
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
    /// `bl list --status|-s ready|blocked|claimed` — the derived ladder (§3) as
    /// a predicate. `None` ⇒ no live filter (every live ball). Only `list`
    /// accepts it; it filters the LIVE rung alone (dead balls left no ladder
    /// behind). The fourth rung, `closed`, carries no live predicate — it steers
    /// `reach` to `Dead` instead (see [`super::flags`]).
    pub status: Option<Status>,
    /// How far into history a listing reaches (§9). `Live` (default) is the
    /// current `tasks/`; `Dead` (`--status closed`) / `All` (`--all`)
    /// reconstruct dead balls most-recent-down. `list`-only.
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
    /// Only the dead set, reconstructed from history (`--status closed`).
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

/// The §8 diffless dispatch for the read verbs: load the store catalog, then
/// render. `edge.color` is the resolved host signal (tty AND no `NO_COLOR`);
/// `--plain` overrides it off. Reads never touch the landing or a remote (§13).
pub fn run(edge: &Edge, verb: Verb, args: &[String]) -> io::Result<()> {
    let flags = parse(verb, args)?;
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    let cat = Catalog::load(&store)?;
    let style = Style { plain: flags.plain || !edge.color };
    let out = match verb {
        Verb::Show => {
            // §6 read dispatch: the HUMAN render folds wired plugins' lines in
            // (the delivery worktree path, §11); `--json` never dispatches — it
            // stays the lossless mirror of stored frontmatter.
            let folded = match &flags.target {
                Some(id) if !flags.json => readop::fold(edge, &store, verb, id),
                _ => String::new(),
            };
            show::dispatch(&store, &cat, &flags, &style, &folded)?
        }
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

    /// The badge for a dead (history-served) ball: a dim `✓` glyph in rich mode,
    /// the padded `closed` word in plain mode — the dead counterpart of
    /// [`Self::badge`] (§9). Every retirement reads as **closed**: a `drop` is a
    /// close without delivery, so the two collapse in projection; the distinguishing
    /// `bl-op:` trailer survives only as git bedrock (§5), never as a status word.
    pub(crate) fn retired_badge(&self) -> String {
        if self.plain {
            return format!("{:<8}", "closed");
        }
        "\u{1b}[90m\u{2713}\u{1b}[0m".to_string() // ✓ dim grey — retired, not live
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
/// every stored field round-trips back to the file, and NOTHING derived appears —
/// no `status` ladder, no ISO dates (timestamps stay the literal stored i64), no
/// inverse-derived `children`, no tree nesting. The derived columns live on the
/// orthogonal HUMAN render alone (§3, bl-d074). `id` is the filename identity
/// (the round-trip key), not a frontmatter field.
///
/// Preserved `extra` keys (§3 seam — a team's `state:` field) ride through too:
/// lossless means EVERY stored key. And ONLY stored keys: a plugin-computed
/// value (the delivery worktree path, §11) is never here — `--json` never
/// dispatches a read-op plugin (§6), it mirrors the file alone. Extras are
/// UNKNOWN keys, so none can collide with the canonical fields layered over
/// them; the canonical set is always present (a cleared scalar emits `null`),
/// unlike the file's skip-if-absent frontmatter.
pub(crate) fn task_json(id: &str, task: &Task) -> Value {
    let blockers: Vec<Value> = task
        .blockers
        .iter()
        .map(|b| json!({ "id": b.id, "on": on_word(b.on) }))
        .collect();
    let mut record = serde_json::to_value(&task.extra).expect("a toml table serializes to a json object");
    let map = record.as_object_mut().expect("a toml table is a json object");
    for (key, value) in [
        ("id", json!(id)),
        ("title", json!(task.title)),
        ("claimant", json!(task.claimant)),
        ("priority", json!(task.priority)),
        ("parent", json!(task.parent)),
        ("tags", json!(task.tags)),
        ("blockers", json!(blockers)),
        ("created", json!(task.created)),
        ("updated", json!(task.updated)),
    ] {
        map.insert(key.to_string(), value);
    }
    record
}

/// Serialise a JSON value to a trailing-newline string — the one place every
/// `--json` branch renders, so the machine contract is byte-identical.
pub(crate) fn json_line(v: &Value) -> String {
    format!("{}\n", serde_json::to_string_pretty(v).expect("serde_json::Value serializes"))
}

#[cfg(test)]
#[path = "reads_tests.rs"]
mod tests;
