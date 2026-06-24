//! The human-output style (§9): status badges as coloured glyphs, or stable
//! glyph-free ASCII under `--plain`/`NO_COLOR`/non-tty. Lifted from [`super`] so
//! the read-verb dispatch stays orchestration; `--json` never renders through it.

use crate::task::Status;

/// The human-output style: glyphs + ANSI colour, or stable glyph-free ASCII.
pub(crate) struct Style {
    pub plain: bool,
}

impl Style {
    /// The status badge for a line: a coloured glyph in rich mode, a padded
    /// lowercase word in plain mode (`--plain`/`NO_COLOR`/non-tty).
    pub(crate) fn badge(&self, s: Status) -> String {
        if self.plain {
            return format!("{:<8}", s.word());
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
    /// [`Self::badge`] (§9). Every retirement reads as **closed** — including a
    /// legacy `drop` deletion (the verb is gone, its history is not); the
    /// `bl-op:` trailer survives only as git bedrock (§5), never as a status word.
    pub(crate) fn retired_badge(&self) -> String {
        if self.plain {
            return format!("{:<8}", "closed");
        }
        "\u{1b}[90m\u{2713}\u{1b}[0m".to_string() // ✓ dim grey — retired, not live
    }
}
