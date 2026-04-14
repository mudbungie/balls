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
        Some(b) if !b.is_empty() => format!("{}\n\n{}", tagged, b),
        _ => tagged,
    }
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
