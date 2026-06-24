//! Read-verb flag parsing — `[--json] [--plain]` for every read, plus the
//! `list`-only filters (§9): `--status`/`-s` (one axis over every §3 rung,
//! `closed` included — it INFERS the dead-set reach), the `--all` reach, and the
//! compose-AND `--tag`/`--since`/`--until` history filters. Every `list` filter
//! is gated on `verb == List`; on any other read it falls through to the
//! unknown-flag arm, so `show` rejects them.

use std::io;

use super::{legacy, Flags, Reach};
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
            "--status" | "-s" if verb == Verb::List => apply_status(&mut f, value(&mut args, "--status")?)?,
            "--all" if verb == Verb::List => set_reach(&mut f, Reach::All)?,
            "--tag" if verb == Verb::List => f.tags.push(value(&mut args, "--tag")?.clone()),
            "--since" if verb == Verb::List => f.since = Some(date(value(&mut args, "--since")?)?),
            // `--until` bounds the whole named day: its last second is inclusive.
            "--until" if verb == Verb::List => f.until = Some(date(value(&mut args, "--until")?)? + 86_399),
            // §16 migration shim — every read accepts it (list = the preview,
            // show = one projected ball); the spec rides `--legacy=REF`.
            arg if legacy::flag(arg).is_some() => f.legacy = legacy::flag(arg),
            flag if flag.starts_with('-') => {
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
    // The legacy store has no greenfield history to reconstruct: `--legacy`
    // serves the LIVE legacy set alone, so a dead-set reach contradicts it.
    if f.legacy.is_some() && f.reach != Reach::Live {
        return Err(io::Error::other("list: --legacy serves the live legacy set — it has no --all/--status closed reach"));
    }
    Ok(f)
}

/// The value following a value-taking flag, or a "needs a value" error naming it.
fn value<'a>(args: &mut std::slice::Iter<'a, String>, flag: &str) -> io::Result<&'a String> {
    args.next().ok_or_else(|| io::Error::other(format!("list: {flag} needs a value")))
}

/// Steer the history reach off its live default, rejecting a second reach
/// request — `--status closed` and `--all` each name one set, so combining them
/// is a contradiction, not a last-wins.
fn set_reach(f: &mut Flags, reach: Reach) -> io::Result<()> {
    if f.reach != Reach::Live {
        return Err(io::Error::other("list: choose one of --status closed / --all"));
    }
    f.reach = reach;
    Ok(())
}

/// Parse a `--since`/`--until` `YYYY-MM-DD` value to its day-start unix second.
fn date(value: &str) -> io::Result<i64> {
    start_of_day(value).ok_or_else(|| io::Error::other(format!("list: bad date '{value}' (want YYYY-MM-DD)")))
}

/// Apply a `--status`/`-s` rung onto the flags. The three live rungs
/// (`ready|blocked|claimed`) narrow the live ladder via [`Flags::status`],
/// parsed by [`Status::from_word`] — derived from the rendered badge, so the
/// filter token can't drift from it. The terminal rung `closed` has no live
/// badge (the file is gone, §2), so it instead INFERS the dead-set reach (§9),
/// folding the retired `--closed` flag into this one status axis.
fn apply_status(f: &mut Flags, value: &str) -> io::Result<()> {
    if value == "closed" {
        return set_reach(f, Reach::Dead);
    }
    f.status = Some(Status::from_word(value).ok_or_else(|| {
        io::Error::other(format!("list: unknown --status '{value}' (want ready|blocked|claimed|closed)"))
    })?);
    Ok(())
}
