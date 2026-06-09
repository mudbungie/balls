//! `bl update --edit` — the $EDITOR-sourced whole-buffer update (§9). The HUMAN
//! projection of `update`: render the stored `tasks/<id>.md` to a temp buffer,
//! block on the host editor, parse-validate the result as a [`Task`] (reusing
//! the one taskfile parse), and hand it back for the EXISTING update seal —
//! never a new verb, never an unvalidated commit. Agents keep the flag-driven
//! path: a non-tty stdin or an unset `$EDITOR` is an error, not a fallback.
//!
//! [`Editor`] is the host seam — env, tty-ness, and the prompt input are
//! captured ONCE at [`Editor::live`] so tests inject a scripted editor and a
//! canned answer stream instead of mutating process env (`env::set_var` races
//! parallel tests).

use std::env;
use std::fs;
use std::io::{self, BufRead, IsTerminal};
use std::path::Path;
use std::process::Command;

use super::other;
use crate::task::Task;

/// The `--edit` host seam: which editor, whether stdin is interactive, and the
/// stream the re-edit prompt reads answers from. Built from the live process at
/// [`Editor::live`]; tests construct it directly with a scripted editor.
pub(crate) struct Editor {
    /// `$EDITOR` (else `$VISUAL`) at invocation; `None` when neither is set.
    pub cmd: Option<String>,
    /// Is stdin a terminal? `--edit` is interactive-only (§9): agents pass
    /// field flags, so a non-tty invocation is refused outright.
    pub tty: bool,
    /// Where the re-edit prompt reads its answer — live stdin in production.
    pub input: Box<dyn BufRead>,
}

impl Editor {
    /// The live host seam — read env and stdin once, in [`super::run`], so the
    /// dispatch below it stays injectable.
    pub(crate) fn live() -> Self {
        Editor {
            cmd: env::var("EDITOR").or_else(|_| env::var("VISUAL")).ok(),
            tty: io::stdin().is_terminal(),
            input: Box::new(io::stdin().lock()),
        }
    }

    /// Run the `--edit` interaction for `id`: write `before` (the stored ball,
    /// rendered) to a temp buffer, block on the editor, and parse-validate the
    /// result. `None` means the buffer parsed back EQUAL to `before` — the
    /// idempotent no-op, announced on stderr, nothing to seal. The buffer is
    /// removed on every exit path.
    pub(super) fn edited(&mut self, before: &Task, id: &str) -> io::Result<Option<Task>> {
        if !self.tty {
            return Err(other("update --edit: stdin is not a tty — pass field flags instead"));
        }
        let Some(cmd) = self.cmd.clone() else {
            return Err(other("update --edit: set $EDITOR (or $VISUAL) or pass field flags"));
        };
        let buffer = env::temp_dir().join(format!("bl-edit-{id}-{}.md", super::change_token()));
        let staged = fs::write(&buffer, before.to_markdown());
        let outcome = staged.and_then(|()| self.cycle(&cmd, &buffer, before, id));
        let _ = fs::remove_file(&buffer);
        outcome
    }

    /// The edit loop: editor → parse → accept, no-op, re-edit, or abort. A
    /// buffer that does not parse back to a [`Task`] (bad TOML, a missing
    /// required field, malformed `[[blockers]]`) is NEVER committed — the
    /// re-edit pass reopens the SAME buffer, so a half-fixed edit is not lost.
    fn cycle(&mut self, cmd: &str, buffer: &Path, before: &Task, id: &str) -> io::Result<Option<Task>> {
        loop {
            run_editor(cmd, buffer)?;
            match Task::parse(&fs::read_to_string(buffer)?) {
                Ok(after) if after == *before => {
                    eprintln!("update {id}: buffer unchanged — nothing to seal");
                    return Ok(None);
                }
                Ok(after) => return Ok(Some(after)),
                Err(e) if self.reedit(&e.to_string())? => {}
                Err(e) => return Err(other(format!("update --edit: rejected — {e}"))),
            }
        }
    }

    /// The rejected-buffer prompt: name the parse error, offer another editor
    /// pass; anything but an explicit `y` (EOF included) aborts.
    fn reedit(&mut self, err: &str) -> io::Result<bool> {
        eprintln!("update --edit: {err}");
        eprint!("re-edit? [y/N] ");
        let mut line = String::new();
        self.input.read_line(&mut line)?;
        Ok(line.trim().eq_ignore_ascii_case("y"))
    }
}

/// Block on the editor over the buffer. `cmd` is whitespace-split — `$EDITOR`
/// conventionally carries args (`code -w`) — with the buffer path appended; a
/// non-zero exit aborts the op (the editor bailed, so the buffer is suspect).
fn run_editor(cmd: &str, buffer: &Path) -> io::Result<()> {
    let mut parts = cmd.split_whitespace();
    let Some(program) = parts.next() else {
        return Err(other("update --edit: $EDITOR is empty — set it or pass field flags"));
    };
    let status = Command::new(program).args(parts).arg(buffer).status()?;
    if !status.success() {
        return Err(other(format!("update --edit: editor '{cmd}' exited {status}")));
    }
    Ok(())
}

#[cfg(test)]
impl Editor {
    /// A detached seam for the flag-driven tests: no tty, no editor, EOF input —
    /// any `--edit` reaching it errors, everything else never touches it.
    pub(crate) fn detached() -> Self {
        Editor { cmd: None, tty: false, input: Box::new(io::empty()) }
    }
}

#[cfg(test)]
#[path = "mutate_edit_tests.rs"]
mod tests;
