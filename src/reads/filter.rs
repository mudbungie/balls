//! The `bl list` compose-AND filters (§9) — tag-subset, date-window, and
//! text-substring, applied UNIFORMLY to the (possibly history-served) row set.
//! Each predicate is independent and ANDed: a row survives only if it satisfies
//! every active filter. An absent filter is a vacuous pass, so the unfiltered
//! `list` keeps its whole set.
//!
//! The predicate reads stored frontmatter alone (title, body, tags, timestamps),
//! so it is identical for a live ball and a reconstructed dead one — the caller
//! supplies the effective `updated` (a live ball's stored `updated`, a dead
//! ball's deletion-commit date, §9).

use super::Flags;
use crate::task::Task;

/// Does `task` survive every active filter? `updated` is the row's effective
/// activity date — its stored `updated` when live, its deletion-commit date when
/// dead (§9) — paired with `created` for the date window.
pub(crate) fn matches(task: &Task, updated: i64, flags: &Flags) -> bool {
    has_tags(task, flags) && in_window(task.created, updated, flags) && has_text(task, flags)
}

/// Every requested `--tag` is present on the ball (AND-subset). No `--tag` ⇒ a
/// vacuous pass (`all` over an empty set is true).
fn has_tags(task: &Task, flags: &Flags) -> bool {
    flags.tags.iter().all(|want| task.tags.contains(want))
}

/// The ball's `created` OR its effective `updated` falls within the `[since,
/// until]` window — so a ball both born and last-touched inside the window, or
/// straddling either edge, is caught. An absent bound is open on that side.
fn in_window(created: i64, updated: i64, flags: &Flags) -> bool {
    let hit = |t: i64| flags.since.is_none_or(|s| t >= s) && flags.until.is_none_or(|u| t <= u);
    hit(created) || hit(updated)
}

/// The `list` text needle ([`Flags::target`]) is a case-insensitive substring of
/// the title or body. No needle ⇒ a vacuous pass.
fn has_text(task: &Task, flags: &Flags) -> bool {
    match flags.target.as_deref() {
        None => true,
        Some(needle) => {
            let needle = needle.to_lowercase();
            task.title.to_lowercase().contains(&needle) || task.body.to_lowercase().contains(&needle)
        }
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
