//! Coverage for the display-boundary sanitizer. Exercises every
//! `is_dangerous` arm and both `inline`/`block` closure branches, and
//! pins behaviour on the concrete attack payloads a hostile GitHub
//! issue could carry once bidirectional sync lands.

use super::{block, inline, MARK};

fn marks(n: usize) -> String {
    std::iter::repeat_n(MARK, n).collect()
}

#[test]
fn clean_text_passes_through_unchanged() {
    assert_eq!(inline(""), "");
    assert_eq!(inline("Fix the auth timeout"), "Fix the auth timeout");
    assert_eq!(block(""), "");
    assert_eq!(block("a normal description"), "a normal description");
}

#[test]
fn legitimate_unicode_is_preserved() {
    // Emoji, accents, CJK — none are control points; both helpers
    // must leave real content alone.
    let s = "café — 日本語 — ✅ rocket 🚀";
    assert_eq!(inline(s), s);
    assert_eq!(block(s), s);
}

#[test]
fn bare_esc_is_neutralized_in_both() {
    assert_eq!(inline("a\x1bb"), format!("a{MARK}b"));
    assert_eq!(block("a\x1bb"), format!("a{MARK}b"));
}

#[test]
fn csi_color_and_cursor_sequences_are_defanged() {
    // The ESC introducer becomes MARK; the residual `[31m` / `[2J`
    // is harmless literal text once it can no longer be a sequence.
    assert_eq!(inline("\x1b[31mRED\x1b[0m"), format!("{MARK}[31mRED{MARK}[0m"));
    assert_eq!(inline("\x1b[2J\x1b[H"), format!("{MARK}[2J{MARK}[H"));
}

#[test]
fn osc_title_hyperlink_and_clipboard_payloads_are_neutralized() {
    // OSC 0 (set window title): ESC + BEL terminator both stripped.
    assert_eq!(inline("\x1b]0;pwned\x07"), format!("{MARK}]0;pwned{MARK}"));
    // OSC 8 hyperlink spoof.
    let link = "\x1b]8;;http://evil\x07click\x1b]8;;\x07";
    assert_eq!(inline(link), format!("{MARK}]8;;http://evil{MARK}click{MARK}]8;;{MARK}"));
    // OSC 52 clipboard poisoning.
    assert_eq!(inline("\x1b]52;c;cm0gLXJm\x07"), format!("{MARK}]52;c;cm0gLXJm{MARK}"));
}

#[test]
fn cr_bs_del_and_c1_csi_are_neutralized() {
    // CR enables overstrike spoofing; 0x9B is a single-byte CSI some
    // terminals honour without an ESC.
    assert_eq!(inline("safe\rEVIL"), format!("safe{MARK}EVIL"));
    assert_eq!(inline("a\x08b"), format!("a{MARK}b"));
    assert_eq!(inline("a\x7fb"), format!("a{MARK}b"));
    assert_eq!(inline("a\u{9b}31mb"), format!("a{MARK}31mb"));
    assert_eq!(inline("a\u{85}\u{9f}b"), format!("a{}b", marks(2)));
}

#[test]
fn inline_neutralizes_tab_and_newline() {
    // A single-line cell has no business holding them; left intact
    // they corrupt column layout as surely as ESC corrupts the tty.
    assert_eq!(inline("a\tb\nc"), format!("a{MARK}b{MARK}c"));
}

#[test]
fn block_preserves_newline_and_tab_only() {
    // Structure survives; every other control point does not.
    assert_eq!(block("para\n\n\tindented"), "para\n\n\tindented");
    assert_eq!(block("a\nb\tc\x1bd\re\x01f"), format!("a\nb\tc{MARK}d{MARK}e{MARK}f"));
}

#[test]
fn boundary_code_points_classify_correctly() {
    // 0x1F last C0 (danger), 0x20 space (safe), 0x7E ~ (safe),
    // 0x7F DEL (danger), 0x9F last C1 (danger), 0xA0 NBSP (safe).
    assert_eq!(inline("\u{1f}\u{20}\u{7e}\u{7f}\u{9f}\u{a0}"), format!("{MARK} ~{MARK}{MARK}\u{a0}"));
    // 0x00 first C0 — the low end of the range arm.
    assert_eq!(inline("x\u{0}y"), format!("x{MARK}y"));
    // block: NBSP is not dangerous and is not the \n/\t exception.
    assert_eq!(block("\u{a0}"), "\u{a0}");
}

#[test]
fn replacement_is_strictly_one_to_one() {
    // The list-truncation math depends on width being preserved.
    let payload = "\x1b]0;t\x07\r\n\t\x7f\u{9b}";
    assert_eq!(inline(payload).chars().count(), payload.chars().count());
}
