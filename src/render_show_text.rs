//! Text helpers for `render_show`: relative timestamps and paragraph
//! word-wrap. Split out so `render_show.rs` stays under the 300-line
//! cap while keeping the helpers testable in isolation.

use super::render_show::Ctx;
use chrono::{DateTime, Utc};

/// Relative timestamp: "3d ago", "2h ago", "5m ago", "just now".
/// `--verbose` appends the absolute ISO for audit trails.
pub fn format_time(ts: DateTime<Utc>, ctx: &Ctx<'_>) -> String {
    let rel = relative_time(ts, ctx.now);
    if ctx.verbose {
        format!("{rel} ({})", ts.to_rfc3339())
    } else {
        rel
    }
}

pub fn relative_time(ts: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let abs = now.signed_duration_since(ts).num_seconds().unsigned_abs();
    if abs < 60 {
        return "just now".into();
    }
    if abs < 3600 {
        return format!("{}m ago", abs / 60);
    }
    if abs < 86_400 {
        return format!("{}h ago", abs / 3600);
    }
    format!("{}d ago", abs / 86_400)
}

/// Word-wrap a paragraph to `width` columns, preserving paragraph
/// breaks. Lines already short pass through unchanged.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in text.split("\n\n") {
        if paragraph.is_empty() {
            out.push(String::new());
            continue;
        }
        out.extend(wrap_line(paragraph, width));
        out.push(String::new());
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

fn wrap_line(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let projected = if current.is_empty() {
            word.len()
        } else {
            current.chars().count() + 1 + word.len()
        };
        if projected > width && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
#[path = "render_show_text_tests.rs"]
mod tests;
