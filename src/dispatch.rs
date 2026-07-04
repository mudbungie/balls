//! §8 dispatch — argv → verb → run, and the two pre-verb help affordances.
//!
//! The crate root ([`crate`]) owns the module map, the branch constants, and the
//! [`crate::usage`] taxonomy bit; this module owns the entrypoint that resolves a
//! command line to its verb and routes it to the right subsystem. [`run`] is
//! re-exported as `balls::run`, the one symbol the `bl` binary calls.

use crate::edge::Edge;
use crate::verb::Verb;
use crate::{checkout, conf, help, import, install, mutate, reads};

/// The agent skill guide, embedded so `bl skill` works from a bare `cargo
/// install` (no repo checkout to read it from). `skill` is help OUTPUT, not an
/// op — it authors no diff, has no lifecycle, and is never a blocker target — so
/// it is dispatched directly in [`run`] and deliberately kept OUT of the [`Verb`]
/// enum (which doubles as a blocker's `on`, §10).
const SKILL: &str = include_str!("../SKILL.md");

/// The §8 dispatch entrypoint: resolve argv to its verb and run it. `prime`/
/// `sync` (§12/§13) wire to the engine via [`checkout`]; the deliverable verbs
/// (§9) via [`mutate`]; the read verbs (`show`/`list`, §9) via
/// [`reads`] — they author no diff and print the store view; `install` (§6)
/// seals its path-copy onto the landing or store via [`install::run`]. `skill`
/// prints the embedded agent guide and `help` (also `--help`/`-h`) the terse
/// command directory ([`help::directory`]). `edge` carries the host inputs `main`
/// resolved.
///
/// Returns the process exit code: `0` on success (including `skill`/`help`), `1`
/// on an op failure (a plugin aborted, a bad flag), `2` for an unknown or missing
/// command (usage convention — the message points at `bl help`).
///
/// `--log-level LEVEL` is the §4 layer-1 CLI override (the only global flag): it
/// is stripped here from anywhere in argv and stamped onto the [`Edge`] the op
/// reads, so the per-verb parsers never see it. A trailing `--log-level` with no
/// value is a usage error (exit 2).
pub fn run(edge: &Edge, args: &[String]) -> i32 {
    let (log_level, rest) = match strip_log_level(args) {
        Ok(split) => split,
        Err(e) => {
            eprintln!("bl: {e}");
            return 2;
        }
    };
    // `skill` (full manual) and `help` (terse command directory) are help OUTPUT,
    // not ops: kept out of `Verb`, dispatched here, print to stdout, exit 0. `help`
    // also answers the conventional `--help`/`-h`.
    match rest.first().map(String::as_str) {
        Some("skill") => {
            print!("{SKILL}");
            return 0;
        }
        // `bl help [<cmd>]`: a known command after `help` gets ITS help (flags +
        // examples); bare `help`/`--help`/`-h` gets the command directory.
        Some("help" | "--help" | "-h") => {
            match rest.get(1).map(String::as_str).and_then(Verb::parse) {
                Some(verb) => print!("{}", help::command(verb)),
                None => print!("{}", help::directory()),
            }
            return 0;
        }
        _ => {}
    }
    let edge = &Edge { log_level, ..edge.clone() };
    let Some(token) = rest.first().map(String::as_str) else {
        eprintln!("usage: bl <command> — run `bl help` for the list");
        return 2;
    };
    let Some(verb) = Verb::parse(token) else {
        eprintln!("bl: unknown command '{token}' — run `bl help` for the list");
        return 2;
    };
    // `bl <cmd> --help` / `-h`: that command's help, before its parser runs (so
    // it works on an unprimed checkout and never needs the verb's positionals). A
    // `--help` past the `--` end-of-options is a positional, not a help request.
    if rest[1..].iter().take_while(|a| *a != "--").any(|a| a == "--help" || a == "-h") {
        print!("{}", help::command(verb));
        return 0;
    }
    let result = match verb {
        Verb::Prime => checkout::prime(edge, &rest[1..]),
        Verb::Sync => checkout::sync(edge, &rest[1..]),
        Verb::Show | Verb::List => reads::run(edge, verb, &rest[1..]),
        // `import` is the write inverse of the bedrock read (§16): records ride
        // stdin, so the host stream is bound here at the edge and injected.
        // UNLOCKED (`Stdin` locks per read): the `--legacy` edge pass re-enters
        // stdin via `mutate::run`'s editor seam, and the std stdin mutex is not
        // reentrant — a lock held across the verb self-deadlocks (bl-0a80).
        Verb::Import => import::run(edge, &mut std::io::stdin(), &rest[1..]),
        Verb::Install => install::run(edge, &rest[1..]),
        Verb::Conf => conf::run(edge, &rest[1..]),
        // Everything left is a deliverable verb (§9); mutate's own dispatch
        // still rejects a non-mutating verb defensively.
        v => mutate::run(edge, v, &rest[1..]),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            // `e` already names the verb where it adds clarity (`claim: … blocked
            // by …`, `show: needs a ball id`); the wrapper just tags it as a bl
            // error, so the verb is named ONCE — not the doubled `bl show: show:`.
            eprintln!("bl: {e}");
            // A USAGE error — the argv was malformed (an unknown flag, a missing
            // value, the wrong positional count) — surfaces the command's flags
            // (bl-7990); an operational failure (a blocked op, a missing ball)
            // stays terse. The [`crate::usage`] tag is the only thing that tells
            // them apart, so the help is offered exactly where it answers.
            if e.kind() == std::io::ErrorKind::InvalidInput {
                eprintln!();
                eprint!("{}", help::command(verb));
            }
            1
        }
    }
}

/// Pull the global `--log-level LEVEL` flag out of argv (from any position),
/// returning the requested level and argv with the flag removed. A `--log-level`
/// with no following value is a usage error.
fn strip_log_level(args: &[String]) -> Result<(Option<String>, Vec<String>), String> {
    let mut level = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--log-level" {
            i += 1;
            level = Some(args.get(i).ok_or("--log-level needs a value")?.clone());
        } else {
            rest.push(args[i].clone());
        }
        i += 1;
    }
    Ok((level, rest))
}

#[cfg(test)]
#[path = "dispatch_test_support.rs"]
mod support;

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dispatch_help_tests.rs"]
mod help_tests;
