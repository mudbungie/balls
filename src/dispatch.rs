//! §8 dispatch — argv → verb → run, and the pre-verb help/skill affordances.
//!
//! The crate root ([`crate`]) owns the module map, the branch constants, and the
//! [`crate::usage`] taxonomy bit; this module owns the entrypoint that resolves a
//! command line to its verb and routes it to the right subsystem. [`run`] is
//! re-exported as `balls::run`, the one symbol the `bl` binary calls.

use crate::edge::Edge;
use crate::verb::Verb;
use crate::{checkout, conf, help, import, install, mutate, reads, skill};

/// Appended to `bl skill`'s output (the bare subcommand spelling only): the
/// subcommand form is on a deprecation path in favor of the flag form `bl
/// --skill`, symmetric with the per-command `bl <cmd> --skill`. Kept working for
/// now — the note is the migration signal, not a removal.
const SKILL_DEPRECATION: &str = "\n\
---\n\
Note: `bl skill` is on a DEPRECATION PATH. Use `bl --skill` for this guide, and\n\
`bl <command> --skill` for a command's full usage (`--help` is an alias).\n";

/// The §8 dispatch entrypoint: resolve argv to its verb and run it. `prime`/
/// `sync` (§12/§13) wire to the engine via [`checkout`]; the deliverable verbs
/// (§9) via [`mutate`]; the read verbs (`show`/`list`, §9) via
/// [`reads`] — they author no diff and print the store view; `install` (§6)
/// seals its path-copy onto the landing or store via [`install::run`].
/// `--skill`/`skill` print the top-level operating guide ([`skill::top`]) and
/// `bl <cmd> --skill`/`--help` a command's full doc ([`skill::command`]); `help`
/// (also `--help`/`-h`) prints the terse command directory ([`help::directory`]).
/// `edge` carries the host inputs `main` resolved.
///
/// Returns the process exit code: `0` on success (including `skill`/`help`), `1`
/// on an op failure (a plugin aborted, a bad flag), `2` for an unknown or missing
/// command (usage convention — the message points at `bl help`).
///
/// Two GLOBAL flags are honoured by every command, stripped here from anywhere
/// in argv and stamped onto the [`Edge`] the op reads, so the per-verb parsers
/// never see them; either one trailing with no value is a usage error (exit 2).
/// `--log-level LEVEL` is the §4 layer-1 CLI override. `-C PATH` (the git/make
/// convention) replaces `invocation_path` verbatim — the substrate is keyed on
/// that path exactly ([`crate::layout::Xdg::clone_dir`]), so `-C` addresses the
/// store keyed by `PATH` with no walking, no git-root discovery, no fallback.
pub fn run(edge: &Edge, args: &[String]) -> i32 {
    let globals = strip_global(args, "--log-level").and_then(|(log_level, rest)| {
        strip_global(&rest, "-C").map(|(dir, rest)| (log_level, dir, rest))
    });
    let (log_level, directory, rest) = match globals {
        Ok(split) => split,
        Err(e) => return usage_error(&e),
    };
    // `skill`/`--skill` (the guide) and `help` (terse command directory) are help
    // OUTPUT, not ops: kept out of `Verb`, dispatched here, print to stdout, exit
    // 0. `--skill` is the canonical spelling (symmetric with `bl <cmd> --skill`);
    // the `skill` subcommand is kept but deprecated (a trailing note). A known
    // command after either spelling gets ITS full doc; bare gets the top guide.
    match rest.first().map(String::as_str) {
        Some("--skill") => {
            match rest.get(1).map(String::as_str).and_then(Verb::parse) {
                Some(verb) => print!("{}", skill::command(verb)),
                None => print!("{}", skill::top()),
            }
            return 0;
        }
        Some("skill") => {
            if let Some(verb) = rest.get(1).map(String::as_str).and_then(Verb::parse) {
                print!("{}", skill::command(verb));
            } else {
                print!("{}", skill::top());
                print!("{SKILL_DEPRECATION}");
            }
            return 0;
        }
        // `bl help [<cmd>]`: a known command after `help` gets ITS full doc (the
        // per-command skill, into which `--help` is folded); bare `help`/`--help`/
        // `-h` gets the terse command directory.
        Some("help" | "--help" | "-h") => {
            match rest.get(1).map(String::as_str).and_then(Verb::parse) {
                Some(verb) => print!("{}", skill::command(verb)),
                None => print!("{}", help::directory()),
            }
            return 0;
        }
        _ => {}
    }
    // `-C` is resolved AFTER the help affordances, so a doc still prints from a
    // bad directory: help output needs no substrate at all.
    let invocation_path = match resolve_directory(directory.as_deref(), &edge.invocation_path) {
        Ok(path) => path,
        Err(e) => return usage_error(&e),
    };
    let edge = &Edge { invocation_path, log_level, ..edge.clone() };
    let Some(token) = rest.first().map(String::as_str) else {
        eprintln!("usage: bl <command> — run `bl help` for the list");
        return 2;
    };
    let Some(verb) = Verb::parse(token) else {
        eprintln!("bl: unknown command '{token}' — run `bl help` for the list");
        return 2;
    };
    // `bl <cmd> --skill` (canonical) / `--help` / `-h`: that command's full doc,
    // before its parser runs (so it works on an unprimed checkout and never needs
    // the verb's positionals). `--help` is folded into `--skill` — one per-command
    // doc, both spellings. A flag past the `--` end-of-options is a positional,
    // not a help request.
    if rest[1..].iter().take_while(|a| *a != "--").any(|a| a == "--skill" || a == "--help" || a == "-h") {
        print!("{}", skill::command(verb));
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
            // value, the wrong positional count) — surfaces the command's tight
            // `usage:` block (its shape + flags, bl-7990) and points at the full
            // doc; an operational failure (a blocked op, a missing ball) stays
            // terse. The [`crate::usage`] tag is the only thing that tells them
            // apart, so the usage is offered exactly where it answers. Not the
            // whole doc — that was too verbose for a mis-invocation.
            if e.kind() == std::io::ErrorKind::InvalidInput {
                eprintln!();
                eprintln!("{}", skill::usage(verb));
                eprintln!("run `bl {} --skill` for flags and examples", verb.token());
            }
            1
        }
    }
}

/// Report a malformed command line and yield the usage exit code (2) — the one
/// shape every pre-verb argv failure takes.
fn usage_error(e: &str) -> i32 {
    eprintln!("bl: {e}");
    2
}

/// Pull a global `<flag> VALUE` pair out of argv (from any position), returning
/// the value and argv with both words removed. The globals are position-
/// independent by construction: they are lifted before the verb's own parser
/// ever runs. A `flag` with no following value is a usage error.
fn strip_global(args: &[String], flag: &str) -> Result<(Option<String>, Vec<String>), String> {
    let mut value = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag {
            i += 1;
            value = Some(args.get(i).ok_or_else(|| format!("{flag} needs a value"))?.clone());
        } else {
            rest.push(args[i].clone());
        }
        i += 1;
    }
    Ok((value, rest))
}

/// Resolve the `-C PATH` override to the op's invocation path: `PATH`
/// canonicalized, or `cwd` untouched when the flag is absent. Canonicalization
/// is the whole of the policy — the store addressed is exactly the one keyed by
/// the resolved path, so a directory with no substrate behaves precisely as if
/// `bl` had been run inside it (a read is silent-empty, `prime` founds). Only a
/// path that is not an existing directory is refused; there is no walking and no
/// git-root discovery to fall back on.
fn resolve_directory(directory: Option<&str>, cwd: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let Some(d) = directory else { return Ok(cwd.to_path_buf()) };
    std::fs::canonicalize(d)
        .ok()
        .filter(|p| p.is_dir())
        .ok_or_else(|| format!("-C {d}: no such directory — -C addresses the store keyed by a path that exists"))
}

#[cfg(test)]
#[path = "dispatch_test_support.rs"]
pub(crate) mod support;

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dispatch_help_tests.rs"]
mod help_tests;
