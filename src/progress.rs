//! Epic completion bar: 10-cell block with closed/total and percent.
//!
//! Only rendered for `type=epic` rows; every other task type hides
//! the bar so it stays a scannable signal rather than noise. The
//! closed count is the union of `closed_children` (archived) and
//! live children with `status=closed`, which is what `ready::completion`
//! already measures — this module exposes the raw counts alongside.

use crate::display::Display;
use crate::ready;
use crate::task::Task;

const BAR_WIDTH: usize = 10;

/// Closed/total for a parent. Closed = archived children plus live
/// children whose status is `closed`. Total = archived + live. A
/// parent with no children returns (0, 0).
pub fn counts(tasks: &[Task], parent_id: &str) -> (usize, usize) {
    let archived = tasks
        .iter()
        .find(|t| t.id == parent_id)
        .map_or(0, |p| p.closed_children.len());
    let live = ready::children_of(tasks, parent_id);
    let live_closed = live
        .iter()
        .filter(|t| matches!(t.status, crate::task::Status::Closed))
        .count();
    let closed = archived + live_closed;
    let total = archived + live.len();
    (closed, total)
}

/// 10-cell block with done/todo glyphs. Unicode: █/░. ASCII: #/-.
/// A 0/0 bar renders entirely as "todo" cells rather than empty —
/// it's still a 10-cell visual anchor so the column aligns.
pub fn bar(closed: usize, total: usize, d: Display) -> String {
    let filled = if total == 0 {
        0
    } else {
        (closed * BAR_WIDTH) / total
    };
    let (done, todo) = if d.use_unicode() { ("█", "░") } else { ("#", "-") };
    let mut s = String::with_capacity(BAR_WIDTH * 3);
    for _ in 0..filled {
        s.push_str(done);
    }
    for _ in filled..BAR_WIDTH {
        s.push_str(todo);
    }
    s
}

/// Full "██████░░░░ 6/10  60%" rendering for inline display.
pub fn summary(closed: usize, total: usize, d: Display) -> String {
    let pct = if total == 0 {
        0
    } else {
        (closed * 100) / total
    };
    format!("{} {closed}/{total}  {pct}%", bar(closed, total, d))
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
