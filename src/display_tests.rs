use super::*;
use crate::task::Status;

// Badge tests live in a sibling file (display_badge_tests.rs) to keep
// this one under the repo's 300-line per-file cap.

// ---------- resolve ----------

#[test]
fn resolve_tty_no_flags_enables_both() {
    let d = Display::resolve(false, false, false, true);
    assert!(d.use_color());
    assert!(d.use_unicode());
}

#[test]
fn resolve_plain_forces_both_off() {
    let d = Display::resolve(true, false, false, true);
    assert!(!d.use_color());
    assert!(!d.use_unicode());
}

#[test]
fn resolve_no_color_disables_both() {
    let d = Display::resolve(false, true, false, true);
    assert!(!d.use_color());
    assert!(!d.use_unicode());
}

#[test]
fn resolve_clicolor_zero_disables_color_only() {
    // CLICOLOR=0 opts out of color; Unicode glyphs stay because the
    // variable is historically about color, not charsets.
    let d = Display::resolve(false, false, true, true);
    assert!(!d.use_color());
    assert!(d.use_unicode());
}

#[test]
fn resolve_non_tty_disables_both() {
    let d = Display::resolve(false, false, false, false);
    assert!(!d.use_color());
    assert!(!d.use_unicode());
}

// ---------- detect ----------

#[test]
fn detect_plain_forces_off_regardless_of_env() {
    let d = Display::detect(true);
    assert!(!d.use_color());
    assert!(!d.use_unicode());
}

#[test]
fn detect_non_plain_is_consistent() {
    let d = Display::detect(false);
    // Under NO_COLOR or no-TTY, color implies unicode (same disabling
    // signals), so color-on-without-unicode is never valid.
    if d.use_color() {
        assert!(d.use_unicode());
    }
}

// ---------- constructors ----------

#[test]
fn plain_and_styled_constructors() {
    assert_eq!(Display::plain(), Display::new(false, false));
    assert_eq!(Display::styled(), Display::new(true, true));
}

// ---------- prio_dot ----------

#[test]
fn prio_dot_ascii_no_color() {
    let d = Display::plain();
    assert_eq!(d.prio_dot(1), "*");
    assert_eq!(d.prio_dot(4), "*");
}

#[test]
fn prio_dot_unicode_no_color_bare_dot() {
    let d = Display::new(false, true);
    assert_eq!(d.prio_dot(1), "●");
}

#[test]
fn prio_dot_styled_emits_ansi_for_every_priority() {
    let d = Display::styled();
    for p in 1..=5u8 {
        let out = d.prio_dot(p);
        assert!(out.contains('\x1b'), "missing ANSI for p={p}");
        assert!(out.contains('●'));
    }
}

// ---------- status_glyph ----------

#[test]
fn status_glyph_ascii_covers_every_variant() {
    let d = Display::plain();
    let cases: [(Status, &str); 7] = [
        (Status::Open, "[ ]"),
        (Status::InProgress, "[>]"),
        (Status::Review, "[?]"),
        (Status::Blocked, "[!]"),
        (Status::Closed, "[x]"),
        (Status::Deferred, "[-]"),
        (Status::Unknown("future".into()), "[.]"),
    ];
    for (s, want) in cases {
        assert_eq!(d.status_glyph(&s), want, "ascii for {}", s.as_str());
    }
}

#[test]
fn status_glyph_unicode_covers_every_variant() {
    let d = Display::styled();
    let cases: [(Status, &str); 7] = [
        (Status::Open, "○"),
        (Status::InProgress, "»"),
        (Status::Review, "?"),
        (Status::Blocked, "⌀"),
        (Status::Closed, "✓"),
        (Status::Deferred, "~"),
        (Status::Unknown("future".into()), "·"),
    ];
    for (s, want) in cases {
        assert_eq!(d.status_glyph(&s), want, "unicode for {}", s.as_str());
    }
}

// ---------- status_word ----------

#[test]
fn status_word_no_color_echoes_as_str() {
    let d = Display::plain();
    for s in [
        Status::Open,
        Status::InProgress,
        Status::Review,
        Status::Blocked,
        Status::Closed,
        Status::Deferred,
        Status::Unknown("x".into()),
    ] {
        assert_eq!(d.status_word(&s), s.as_str());
    }
}

#[test]
fn status_word_styled_colors_known_and_passes_unknown_through() {
    let d = Display::styled();
    for s in [
        Status::Open,
        Status::InProgress,
        Status::Review,
        Status::Blocked,
        Status::Closed,
        Status::Deferred,
    ] {
        let out = d.status_word(&s);
        assert!(out.contains('\x1b'), "missing ANSI for {}", s.as_str());
        assert!(out.contains(s.as_str()));
    }
    // Unknown has no policy color; render plain to avoid inventing one.
    assert_eq!(d.status_word(&Status::Unknown("future".into())), "future");
}

// ---------- tree_prefix ----------

#[test]
fn tree_prefix_depth_zero_is_empty() {
    assert_eq!(Display::styled().tree_prefix(0, true, &[]), "");
    assert_eq!(Display::plain().tree_prefix(0, false, &[true]), "");
}

#[test]
fn tree_prefix_depth_one_unicode() {
    let d = Display::styled();
    assert_eq!(d.tree_prefix(1, true, &[]), "└─ ");
    assert_eq!(d.tree_prefix(1, false, &[]), "├─ ");
}

#[test]
fn tree_prefix_depth_one_ascii() {
    let d = Display::plain();
    assert_eq!(d.tree_prefix(1, true, &[]), "`- ");
    assert_eq!(d.tree_prefix(1, false, &[]), "|- ");
}

#[test]
fn tree_prefix_nested_renders_pad_and_vbar() {
    // depth=3, ancestors=[not-last, last] → vbar then pad then corner.
    let styled = Display::styled();
    assert_eq!(styled.tree_prefix(3, true, &[false, true]), "│     └─ ");
    let ascii = Display::plain();
    assert_eq!(ascii.tree_prefix(3, false, &[false, true]), "|     |- ");
}

// ---------- init + global ----------
//
// This is the ONLY test that touches the OnceLock, to avoid ordering
// races with other unit tests. `init(true)` maps to plain regardless
// of env, so the post-init state is stable.

#[test]
fn init_and_global_protocol() {
    // Pre-init fallback path is exercised at least once per process
    // boot (this test runs first or later; either way the branch is
    // reached by at least one `global()` call in some coverage run).
    let _ = global();

    init(true);
    let after = global();
    assert!(!after.use_color());
    assert!(!after.use_unicode());

    // set-once: second init is a no-op.
    init(false);
    assert_eq!(global(), after);
}
