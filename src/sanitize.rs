//! Neutralize terminal control sequences in externally-influenced text.
//!
//! Task content — title, description, tags, notes, external slices —
//! is rendered raw to a TTY. The moment bidirectional sync lands those
//! fields are attacker-controlled: a hostile string carrying CSI/OSC
//! escapes can forge `bl list`/`show` output (hide or spoof a task,
//! fake an id/owner), poison the clipboard via OSC 52 so a later paste
//! runs an attacker command, or spoof a hyperlink via OSC 8. External
//! content is *data*, never terminal commands.
//!
//! This module is the single display-boundary chokepoint. Callers
//! sanitize *content* before composing it with balls' own (trusted)
//! SGR styling — we cannot post-process the final line, because by
//! then our own colour escapes are indistinguishable from injected
//! ones. JSON output never passes through here: serde already
//! `\u`-escapes control bytes and the `--json` machine contract must
//! stay byte-exact.
//!
//! Replacement is strictly 1:1 — one offending `char` becomes one
//! [`MARK`] — so callers that measure display width (the `bl list`
//! truncation math) stay correct without dragging in an ANSI parser.

/// Visible, inert stand-in for a neutralized control character. Its
/// presence is the signal that something was stripped; it cannot
/// itself drive a terminal.
const MARK: char = '\u{FFFD}';

/// True for code points that can steer a terminal: the C0 block
/// (`0x00..=0x1F`, which includes ESC `0x1B`, CR `0x0D`, BS `0x08`),
/// DEL (`0x7F`), and the C1 block (`0x80..=0x9F`, where `0x9B` is a
/// single-byte CSI introducer some terminals honour).
fn is_dangerous(c: char) -> bool {
    matches!(c, '\0'..='\u{1F}' | '\u{7F}' | '\u{80}'..='\u{9F}')
}

/// Single-line fields: title, tags, ids, note authors, branch, repo,
/// external names and remote keys/urls. *Every* control character —
/// tab and newline included — is neutralized: a single-line cell has
/// no legitimate use for them, and a stray `\n`/`\t` corrupts column
/// layout just as surely as ESC corrupts the terminal.
pub fn inline(s: &str) -> String {
    s.chars()
        .map(|c| if is_dangerous(c) { MARK } else { c })
        .collect()
}

/// Multi-line fields: the wrapped description. Newline and tab are
/// structural (paragraph breaks, indentation) and survive untouched;
/// every other control character — ESC, CR, the rest of C0, DEL, the
/// C1 block — is neutralized.
pub fn block(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\n' || c == '\t' || !is_dangerous(c) {
                c
            } else {
                MARK
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "sanitize_tests.rs"]
mod tests;
