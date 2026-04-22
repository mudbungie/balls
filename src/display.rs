//! CLI display primitives: colors, glyphs, badges, tree prefixes.
//!
//! # Library choice: owo-colors
//!
//! Picked `owo-colors` over `anstream` for the display foundation:
//!
//! - **Zero runtime deps.** owo-colors is a pure-Rust trait facade over
//!   ANSI escapes. Our TTY detection already lives in std via
//!   `std::io::IsTerminal`, so we do not need anstream's terminal
//!   auto-configuration.
//! - **Composability.** Every styled value is just a wrapper that
//!   implements `Display`, so callers can still format with `{:<12}`
//!   when they want to (at their own risk — ANSI bytes don't carry
//!   visual width).
//! - **No Windows console rewrite.** Windows 10+ handles ANSI natively,
//!   and our target audience is developers on modern shells.
//!
//! # Detection model
//!
//! Detection is pure: `Display::resolve(plain, no_color, clicolor_zero,
//! is_tty)` collapses four bits into a `Display`. `Display::detect(plain)`
//! reads the real environment (`NO_COLOR`, `CLICOLOR`, stdout TTY) and
//! delegates to `resolve`. Consumers pass a `Display` around; the global
//! `OnceLock` behind `init`/`global` exists only so `main` can set it
//! once and command handlers can grab it without plumbing every layer.
//!
//! Glyphs are always paired with status words at the call site — users
//! should never have to memorize an icon. Colors are additive; dropping
//! them must not change meaning. JSON paths bypass this module entirely.

use crate::task::{LinkType, Status, Task};
use owo_colors::OwoColorize;
use std::io::IsTerminal;
use std::sync::OnceLock;

static GLOBAL: OnceLock<Display> = OnceLock::new();

/// Rendered styling decisions. Cheap to copy; passed by value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Display {
    color: bool,
    unicode: bool,
}

impl Display {
    pub fn new(color: bool, unicode: bool) -> Self {
        Display { color, unicode }
    }

    /// Convenience: both features off. Safe default for tests and for
    /// the pre-`init` fallback in `global`.
    pub fn plain() -> Self {
        Display::new(false, false)
    }

    /// Convenience: both features on. Used by tests that assert the
    /// styled-path branches.
    pub fn styled() -> Self {
        Display::new(true, true)
    }

    /// Collapse four detection inputs into a `Display`. Pure.
    ///
    /// * `plain` — user passed `--plain`; force both off.
    /// * `no_color` — `NO_COLOR` present; disables color *and* unicode
    ///   (users who opt out of color typically also want ASCII glyphs).
    /// * `clicolor_zero` — `CLICOLOR=0`; disables color only.
    /// * `is_tty` — stdout is a terminal; required for either.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn resolve(plain: bool, no_color: bool, clicolor_zero: bool, is_tty: bool) -> Self {
        let color = !plain && !no_color && !clicolor_zero && is_tty;
        let unicode = !plain && !no_color && is_tty;
        Display { color, unicode }
    }

    /// Detect from the live environment and stdout TTY state.
    pub fn detect(plain: bool) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let clicolor_zero = std::env::var("CLICOLOR")
            .map(|v| v == "0")
            .unwrap_or(false);
        let is_tty = std::io::stdout().is_terminal();
        Self::resolve(plain, no_color, clicolor_zero, is_tty)
    }

    pub fn use_color(&self) -> bool {
        self.color
    }

    pub fn use_unicode(&self) -> bool {
        self.unicode
    }

    /// Priority marker: `●` (or `*` in ASCII), colored red/yellow/blue/dim
    /// for priorities 1..=4. Unknown priorities fall through to dim.
    pub fn prio_dot(&self, p: u8) -> String {
        let dot = if self.unicode { "●" } else { "*" };
        if !self.color {
            return dot.to_string();
        }
        match p {
            1 => dot.red().to_string(),
            2 => dot.bright_yellow().to_string(),
            3 => dot.blue().to_string(),
            _ => dot.bright_black().to_string(),
        }
    }

    /// One-character status glyph. Always paired with `status_word` at
    /// the call site — this is a scannable anchor, not a replacement.
    pub fn status_glyph(&self, s: &Status) -> &'static str {
        if self.unicode {
            match s {
                Status::Open => "○",
                Status::InProgress => "»",
                Status::Review => "?",
                Status::Blocked => "⌀",
                Status::Closed => "✓",
                Status::Deferred => "~",
                Status::Unknown(_) => "·",
            }
        } else {
            match s {
                Status::Open => "[ ]",
                Status::InProgress => "[>]",
                Status::Review => "[?]",
                Status::Blocked => "[!]",
                Status::Closed => "[x]",
                Status::Deferred => "[-]",
                Status::Unknown(_) => "[.]",
            }
        }
    }

    /// Colored status word. Returns the literal `Status::as_str` when
    /// color is off, so the value is always safe to embed in output.
    pub fn status_word(&self, s: &Status) -> String {
        let w = s.as_str();
        if !self.color {
            return w.to_string();
        }
        match s {
            Status::Open => w.dimmed().to_string(),
            Status::InProgress => w.yellow().to_string(),
            Status::Review => w.cyan().to_string(),
            Status::Blocked => w.magenta().to_string(),
            Status::Closed => w.green().to_string(),
            Status::Deferred => w.bright_black().to_string(),
            Status::Unknown(_) => w.to_string(),
        }
    }

    /// `★` (or `*`) when the task is claimed by the identity we render
    /// on behalf of. Empty string otherwise so the badge slot collapses.
    pub fn claimed_badge(&self, t: &Task, me: &str) -> &'static str {
        match &t.claimed_by {
            Some(c) if c == me => {
                if self.unicode {
                    "★"
                } else {
                    "*"
                }
            }
            _ => "",
        }
    }

    /// `◆` (or `D`) when any declared dependency is still open. Closed
    /// deps are considered met regardless of archival state.
    pub fn deps_badge(&self, t: &Task, all: &[Task]) -> &'static str {
        let unmet = t.depends_on.iter().any(|dep_id| {
            all.iter()
                .any(|o| &o.id == dep_id && !matches!(o.status, Status::Closed))
        });
        if unmet {
            if self.unicode {
                "◆"
            } else {
                "D"
            }
        } else {
            ""
        }
    }

    /// `⛓` (or `G`) when the task has an unresolved `gates` link
    /// blocking its own close.
    pub fn gates_badge(&self, t: &Task, all: &[Task]) -> &'static str {
        let open = t.links.iter().any(|l| {
            matches!(l.link_type, LinkType::Gates)
                && all
                    .iter()
                    .any(|o| o.id == l.target && !matches!(o.status, Status::Closed))
        });
        if open {
            if self.unicode {
                "⛓"
            } else {
                "G"
            }
        } else {
            ""
        }
    }

    /// Box-drawing prefix for a node at `depth`, given whether it is the
    /// last child at its level and the last-ness of each ancestor above
    /// it. `ancestors.len()` must equal `depth - 1`; the root prints no
    /// prefix.
    pub fn tree_prefix(&self, depth: usize, is_last: bool, ancestors: &[bool]) -> String {
        if depth == 0 {
            return String::new();
        }
        let (vbar, corner, tee, pad) = if self.unicode {
            ("│  ", "└─ ", "├─ ", "   ")
        } else {
            ("|  ", "`- ", "|- ", "   ")
        };
        let mut s = String::new();
        for &anc_last in ancestors {
            s.push_str(if anc_last { pad } else { vbar });
        }
        s.push_str(if is_last { corner } else { tee });
        s
    }
}

/// Install the global `Display` from a parsed `--plain` flag. Call once
/// from `main` before dispatching commands. Subsequent calls are no-ops.
pub fn init(plain: bool) {
    let _ = GLOBAL.set(Display::detect(plain));
}

/// Read the installed global. Returns `Display::plain` if `init` has
/// not run yet — a conservative default that keeps early output from
/// accidentally emitting ANSI escapes.
pub fn global() -> Display {
    GLOBAL.get().copied().unwrap_or_else(Display::plain)
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "display_badge_tests.rs"]
mod badge_tests;
