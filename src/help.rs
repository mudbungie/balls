//! `bl help` — the minimal command DIRECTORY. The terse companion to the fuller
//! `bl --skill` guide: `--skill` carries the depth (what, and in decreasing
//! quantity how and why), `help` is just the "what" — one line per command, so a
//! reader can find the verb and then reach for `bl <command> --skill` for its
//! full usage.
//!
//! Like `skill`, `help` is help OUTPUT, not an op (no diff, no lifecycle, never a
//! blocker target), so it is dispatched directly in [`crate::run`] and kept out
//! of the [`Verb`] enum. The directory is GENERATED from [`Verb::ALL`] plus the
//! two non-verb help commands, so it can never list a command the parser does not
//! know nor omit one it does. The per-command depth lives in [`crate::skill`] —
//! `bl <cmd> --help` folds into `bl <cmd> --skill` (one doc, both spellings).

use std::fmt::Write;

use crate::verb::Verb;

/// The help OUTPUTS that are not verbs (dispatched directly in [`crate::run`]),
/// listed in the directory alongside the real commands.
const META: [(&str, &str); 2] = [
    ("skill", "print the operating guide (prefer the flag form: bl --skill)"),
    ("help", "print this command directory"),
];

/// Render the command directory: a one-line header, then one column-aligned line
/// per command (every [`Verb`] then the [`META`] help commands), then the flags
/// shared across commands and the pointers to the deeper docs. Printed to stdout
/// by `bl help` / `bl --help` / `bl -h`.
pub fn directory() -> String {
    // `fold` (not `max`) keeps this infallible — no empty-iterator `Option` to
    // unwrap, so no panic path to document.
    let width = Verb::ALL
        .iter()
        .map(|v| v.token().len())
        .chain(META.iter().map(|(token, _)| token.len()))
        .fold(0, usize::max)
        + 2;
    let mut out = String::from("bl — a git-native task tracker.\n\nusage: bl <command> [args]\n\n");
    for v in Verb::ALL {
        let _ = writeln!(out, "  {:<width$}{}", v.token(), v.summary());
    }
    out.push('\n');
    for (token, summary) in META {
        let _ = writeln!(out, "  {token:<width$}{summary}");
    }
    out.push_str("\nCommon flags: --json (machine-readable output), --as ID (worker identity).\n");
    out.push_str("Full usage for one command: bl <command> --skill (--help is an alias).\n");
    out.push_str("The operating guide (architecture + invariants): bl --skill.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directory_lists_every_verb_with_its_summary() {
        let dir = directory();
        for v in Verb::ALL {
            assert!(dir.contains(v.token()), "{} missing from the directory", v.token());
            assert!(dir.contains(v.summary()), "{}'s summary missing", v.token());
        }
    }

    #[test]
    fn the_directory_lists_the_non_verb_help_commands() {
        let dir = directory();
        // `skill` and `help` are not verbs but belong in the command directory.
        for (token, summary) in META {
            assert!(dir.contains(token), "{token} missing from the directory");
            assert!(dir.contains(summary), "{token}'s summary missing");
        }
        assert!(dir.starts_with("bl —"), "leads with the one-line header");
    }

    #[test]
    fn the_directory_points_at_the_deeper_docs() {
        // The terse directory is a signpost: it must send the reader to the
        // per-command `--skill` and the top-level guide, the folded depth.
        let dir = directory();
        assert!(dir.contains("bl <command> --skill"), "points at per-command usage");
        assert!(dir.contains("bl --skill"), "points at the operating guide");
    }
}
