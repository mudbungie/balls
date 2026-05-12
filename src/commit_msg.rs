//! Format a review-time commit message in the standard git 50/72
//! shape: a single title line with the delivery tag appended, a blank
//! line, then the optional body. Keeps `git log --oneline` readable.

/// Format a squash-merge commit message for a reviewed task.
///
/// The caller's `message` may be:
/// - `None`: fall back to `task_title` as the title. No body.
/// - single-line: becomes the title. No body.
/// - multi-line: first line becomes the title, everything after the
///   first newline becomes the body (with a blank line separator
///   inserted if the caller didn't already provide one).
///
/// The delivery tag `[id]` is always appended to the title so `git
/// log --grep` can locate the commit.
pub fn format_squash(message: Option<&str>, task_title: &str, id: &str) -> String {
    let raw = message.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(task_title);
    let (title, body) = split_title_body(raw);
    let tagged = format!("{} [{}]", title.trim_end(), id);
    match body {
        Some(b) if !b.is_empty() => format!("{tagged}\n\n{b}"),
        _ => tagged,
    }
}

/// Collapse repeated `-m` values into one message string.
///
/// Mirrors `git commit -m … -m …`: each value becomes its own
/// paragraph, joined by a blank line. Whitespace-only values are
/// dropped. Returns `None` when nothing usable remains, so callers
/// fall back to the task title exactly as a missing `-m` would.
pub fn join_messages(parts: &[String]) -> Option<String> {
    let joined = parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!joined.is_empty()).then_some(joined)
}

fn split_title_body(s: &str) -> (&str, Option<String>) {
    let s = s.trim_end_matches('\n');
    let Some(nl) = s.find('\n') else {
        return (s, None);
    };
    let title = &s[..nl];
    // Skip any blank lines between title and body so the caller's
    // input is robust against `"title\nbody"` *or* `"title\n\nbody"`.
    // trim_end above removed trailing newlines globally, so the
    // remainder always contains at least one non-newline character.
    let body = s[nl + 1..].trim_start_matches('\n').to_string();
    (title, Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_uses_task_title() {
        assert_eq!(
            format_squash(None, "Add feature", "bl-abcd"),
            "Add feature [bl-abcd]"
        );
    }

    #[test]
    fn single_line_message_is_title_only() {
        assert_eq!(
            format_squash(Some("Short title"), "ignored", "bl-1234"),
            "Short title [bl-1234]"
        );
    }

    #[test]
    fn title_and_body_split_on_first_newline() {
        let out = format_squash(
            Some("Short title\nExplanation paragraph."),
            "ignored",
            "bl-1234",
        );
        assert_eq!(out, "Short title [bl-1234]\n\nExplanation paragraph.");
    }

    #[test]
    fn title_and_body_with_existing_blank_line_preserved() {
        let out = format_squash(
            Some("Short title\n\nExplanation paragraph."),
            "ignored",
            "bl-1234",
        );
        assert_eq!(out, "Short title [bl-1234]\n\nExplanation paragraph.");
    }

    #[test]
    fn multi_paragraph_body_is_preserved() {
        let out = format_squash(
            Some("Short title\n\nFirst paragraph.\n\nSecond paragraph."),
            "ignored",
            "bl-1234",
        );
        assert_eq!(
            out,
            "Short title [bl-1234]\n\nFirst paragraph.\n\nSecond paragraph."
        );
    }

    #[test]
    fn trailing_newlines_stripped() {
        let out = format_squash(Some("Short title\n\n"), "ignored", "bl-1234");
        assert_eq!(out, "Short title [bl-1234]");
    }

    #[test]
    fn join_messages_none_when_empty_or_blank() {
        assert_eq!(join_messages(&[]), None);
        assert_eq!(join_messages(&["   ".into(), "\n".into()]), None);
    }

    #[test]
    fn join_messages_single_value_passes_through() {
        assert_eq!(join_messages(&["fix foo".into()]).as_deref(), Some("fix foo"));
    }

    #[test]
    fn join_messages_repeated_values_become_paragraphs() {
        let parts = vec!["Short title".into(), "First para.".into(), "Second para.".into()];
        assert_eq!(
            join_messages(&parts).as_deref(),
            Some("Short title\n\nFirst para.\n\nSecond para.")
        );
    }

    #[test]
    fn join_messages_drops_blanks_between_real_values() {
        let parts = vec!["Title".into(), "  ".into(), "Body.".into()];
        assert_eq!(join_messages(&parts).as_deref(), Some("Title\n\nBody."));
    }

    #[test]
    fn repeated_m_then_format_squash_yields_50_72_shape() {
        let msg = join_messages(&["Short title".into(), "Body paragraph.".into()]);
        assert_eq!(
            format_squash(msg.as_deref(), "ignored", "bl-1234"),
            "Short title [bl-1234]\n\nBody paragraph."
        );
    }

    #[test]
    fn empty_message_falls_back_to_task_title() {
        assert_eq!(
            format_squash(Some(""), "Fallback title", "bl-abcd"),
            "Fallback title [bl-abcd]"
        );
        assert_eq!(
            format_squash(Some("   "), "Fallback title", "bl-abcd"),
            "Fallback title [bl-abcd]"
        );
    }
}
