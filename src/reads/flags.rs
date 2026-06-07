//! Read-verb flag parsing — `[--json] [--plain]` for every read, plus the
//! `list`-only filters (§9): `--status`, the `--closed`/`--all` reach, and the
//! compose-AND `--tag`/`--since`/`--until` history filters. Every `list` filter
//! is gated on `verb == List`; on any other read it falls through to the
//! unknown-flag arm, so `show`/`dep-tree` reject them.

use std::io;

use super::{Flags, Reach};
use crate::civil::start_of_day;
use crate::task::Status;
use crate::verb::Verb;

/// Parse a read verb's argv into [`Flags`]. `show` requires its `TARGET` id;
/// `list` accepts the §9 filter family; every read rejects an unknown flag and
/// accepts at most one positional (a ball id, or `list`'s text needle).
pub(crate) fn parse(verb: Verb, args: &[String]) -> io::Result<Flags> {
    let mut f = Flags::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => f.json = true,
            "--plain" => f.plain = true,
            "--status" if verb == Verb::List => f.status = Some(parse_status(value(&mut args, "--status")?)?),
            "--closed" if verb == Verb::List => set_reach(&mut f, Reach::Dead)?,
            "--all" if verb == Verb::List => set_reach(&mut f, Reach::All)?,
            "--tag" if verb == Verb::List => f.tags.push(value(&mut args, "--tag")?.clone()),
            "--since" if verb == Verb::List => f.since = Some(date(value(&mut args, "--since")?)?),
            // `--until` bounds the whole named day: its last second is inclusive.
            "--until" if verb == Verb::List => f.until = Some(date(value(&mut args, "--until")?)? + 86_399),
            flag if flag.starts_with("--") => {
                return Err(io::Error::other(format!("{}: unexpected flag '{flag}'", verb.token())));
            }
            _ => {
                if f.target.replace(arg.clone()).is_some() {
                    return Err(io::Error::other(format!("{}: at most one argument", verb.token())));
                }
            }
        }
    }
    if verb == Verb::Show && f.target.is_none() {
        return Err(io::Error::other("show: needs a ball id"));
    }
    Ok(f)
}

/// The value following a value-taking flag, or a "needs a value" error naming it.
fn value<'a>(args: &mut std::slice::Iter<'a, String>, flag: &str) -> io::Result<&'a String> {
    args.next().ok_or_else(|| io::Error::other(format!("list: {flag} needs a value")))
}

/// Set the history reach, rejecting a second reach flag — `--closed` and `--all`
/// name one set apiece, so combining them is a contradiction, not a last-wins.
fn set_reach(f: &mut Flags, reach: Reach) -> io::Result<()> {
    if f.reach != Reach::Live {
        return Err(io::Error::other("list: choose one of --closed / --all"));
    }
    f.reach = reach;
    Ok(())
}

/// Parse a `--since`/`--until` `YYYY-MM-DD` value to its day-start unix second.
fn date(value: &str) -> io::Result<i64> {
    start_of_day(value).ok_or_else(|| io::Error::other(format!("list: bad date '{value}' (want YYYY-MM-DD)")))
}

/// Parse a `--status` value into its §3 ladder rung — the inverse of
/// [`super::status_word`], so the filter token matches the rendered badge word.
fn parse_status(value: &str) -> io::Result<Status> {
    match value {
        "ready" => Ok(Status::Ready),
        "blocked" => Ok(Status::Blocked),
        "claimed" => Ok(Status::Claimed),
        other => Err(io::Error::other(format!(
            "list: unknown --status '{other}' (want ready|blocked|claimed)"
        ))),
    }
}
