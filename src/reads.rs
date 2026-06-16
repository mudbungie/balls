//! §9 read verbs — `show`, `list`. Diffless ops (§8 "skip
//! steps 1/3/5"): they author no ball-file diff and seal nothing; the printed
//! output IS the whole contribution. Every read's human render may also FOLD
//! IN a §6 read-op plugin dispatch ([`readop`] — `show`'s delivery worktree
//! line, §11); `--json` never dispatches. Each walks `tasks/` on the STORE checkout
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

use std::io;
use std::path::Path;

use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::log::{self, Level, Log};
use crate::task::Status;
use crate::verb::Verb;

mod catalog;
mod filter;
mod flags;
mod history;
pub(crate) mod legacy;
mod list;
mod readop;
mod record;
mod show;
mod style;

pub(crate) use catalog::{Catalog, Entry};
pub(crate) use flags::parse;
pub(crate) use history::resolve_dead;
pub(crate) use record::{json_line, task_json};
pub(crate) use style::Style;

#[cfg(test)]
pub(crate) mod test_support;

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
    /// `--legacy[=REF]` (§16): point this read at the PRE-greenfield JSON store
    /// instead of `tasks/` — the bounded migration shim, projected into the
    /// greenfield wire shape by [`legacy`]. `Some` holds the `<ref>:<dir>` spec
    /// (default `balls/tasks:.balls/tasks`). Severable: delete the flag and the
    /// [`legacy`] module and nothing in core changes.
    pub legacy: Option<String>,
    /// The lone positional: a ball id for `show`, the text-search needle for
    /// `list` (substring over title+body, §9).
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

/// The §8 diffless dispatch for the read verbs: resolve the §4 `log_level`
/// stack (CLI ▸ XDG ▸ landing ▸ default), narrate the read at `debug` (§4 —
/// core narration is `debug` on every op, so the default threshold keeps
/// routine chatter out of the log), then render. `edge.color` is
/// the resolved host signal (tty AND no `NO_COLOR`); `--plain` overrides it
/// off. Reads never touch the landing's git or a remote (§13) — the landing
/// CONFIG is read (the threshold, the `[hooks]` schedule), like every op.
pub fn run(edge: &Edge, verb: Verb, args: &[String]) -> io::Result<()> {
    let flags = parse(verb, args)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let cfg = EffectiveConfig::resolve(&clone.landing(), &edge.xdg.user_config())?;
    let level = Level::parse(edge.log_level.as_deref().unwrap_or(&cfg.log_level))?;
    let log = Log::new(clone.op_log(), level, verb, log::wall);
    log.record(Level::Debug, "core", None, "begin");
    match render(edge, verb, &flags, &clone.store(), &cfg, &log) {
        Ok(out) => {
            print!("{out}");
            log.record(Level::Debug, "core", None, "done");
            Ok(())
        }
        // The whole read lifecycle narrates at `debug` (§4) — abort included: a
        // read mutates nothing, and the error itself reaches the invoker anyway.
        Err(e) => {
            log.record(Level::Debug, "core", None, &format!("abort {e}"));
            Err(e)
        }
    }
}

/// Render one read verb: load the store catalog, fold in the §6 read-op
/// dispatch, render. Reads are not special-cased (§6): EVERY read verb's bare
/// `<op>` hook key dispatches through [`readop::fold`] — `show` ships wired by
/// default, `list` is plugin-free only because nothing is listed for it by
/// default. `--json` never dispatches — it stays the lossless mirror of stored
/// frontmatter; only `show` names a ball on the wire (`metadata.bl-id`), the
/// target-free reads carry no id.
fn render(edge: &Edge, verb: Verb, flags: &Flags, store: &Path, cfg: &EffectiveConfig, log: &Log) -> io::Result<String> {
    // `--legacy` swaps the SOURCE only (§16): the catalog comes from the legacy
    // ref projected into greenfield shape, and everything downstream — status
    // ladder, filters, both renders — is the ordinary read path.
    let cat = match &flags.legacy {
        Some(spec) => Catalog::from_pairs(legacy::balls(&edge.invocation_path, spec)?),
        None => Catalog::load(store)?,
    };
    let style = Style { plain: flags.plain || !edge.color };
    let fold =
        |id: Option<&str>| if flags.json { String::new() } else { readop::fold(edge, store, verb, id, cfg, log) };
    match verb {
        Verb::Show => show::dispatch(store, &cat, flags, &style, &fold(flags.target.as_deref())),
        Verb::List => {
            // The dead set is reconstructed from history only when the reach
            // calls for it — the live-only default never touches git (§9).
            let dead = if flags.reach.dead() { history::dead_balls(store, &cat)? } else { Vec::new() };
            Ok(list::render_list(&cat, &dead, flags, &style) + &fold(None))
        }
        other => Err(io::Error::other(format!("{}: not a read verb", other.token()))),
    }
}

#[cfg(test)]
#[path = "reads_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reads/run_tests.rs"]
mod run_tests;
