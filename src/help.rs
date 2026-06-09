//! `bl help` — the minimal command DIRECTORY. The terse companion to `bl skill`:
//! `skill` is the full manual (what, and in decreasing quantity how and why);
//! `help` is just the "what" — one line per command, so a reader can find the
//! verb and then reach for `skill` for the depth.
//!
//! Like `skill`, `help` is help OUTPUT, not an op (no diff, no lifecycle, never a
//! blocker target), so it is dispatched directly in [`crate::run`] and kept out
//! of the [`Verb`] enum. The directory is GENERATED from [`Verb::ALL`] plus the
//! two non-verb help commands, so it can never list a command the parser does not
//! know nor omit one it does.

use std::fmt::Write;

use crate::verb::Verb;

/// The help OUTPUTS that are not verbs (dispatched directly in [`crate::run`]),
/// listed in the directory alongside the real commands.
const META: [(&str, &str); 2] = [
    ("skill", "print the full agent guide (the complete manual)"),
    ("help", "print this command directory"),
];

/// Render the command directory: a one-line header, then one column-aligned line
/// per command (every [`Verb`] then the [`META`] help commands), then the flags
/// shared across commands. Printed to stdout by `bl help` / `bl --help` / `bl -h`.
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
}
