//! `doctor`'s `.balls/tasks` convenience-symlink probe (bl-38dd's
//! invariant). Lifted out of `doctor.rs` for line-budget. Read-only:
//! flags drift, hints at the recreate path. Re-materialization itself
//! belongs to `state_repo::ensure`; bl-773e is the matching repair
//! side for the legacy→master_url remaster stale-target case.

use crate::doctor::Finding;
use std::fs;
use std::path::Path;

/// Validate `<root>/.balls/tasks`. Expected: a symlink whose
/// `read_link` target equals `expected` (a path relative to `.balls/`).
/// Surfaces missing, stray non-symlink, and stale target. Doctor never
/// mutates — every fix hint goes through `bl remaster <hub-url>
/// --commit` because that is the only flow today that re-runs
/// `state_repo::ensure` against an already-materialized state-repo.
pub(crate) fn check_tasks_symlink(root: &Path, expected: &str, out: &mut Vec<Finding>) {
    let link = root.join(".balls/tasks");
    if !link.is_symlink() {
        out.push(missing_or_stray(&link));
        return;
    }
    let target = fs::read_link(&link).unwrap_or_default();
    let target_str = target.to_string_lossy();
    if target_str != expected {
        out.push(Finding::flag(
            format!("{} points to `{}`, expected `{}`", link.display(), target_str, expected),
            format!(
                "remove {} and re-run `bl remaster <hub-url> --commit` to repoint it",
                link.display()
            ),
        ));
    }
}

fn missing_or_stray(link: &Path) -> Finding {
    if link.exists() {
        Finding::flag(
            format!("{} is not a symlink (stray file or directory)", link.display()),
            format!("remove {} and re-run `bl remaster <hub-url> --commit`", link.display()),
        )
    } else {
        Finding::flag(
            format!("{} convenience symlink is missing", link.display()),
            "re-run `bl remaster <hub-url> --commit` to recreate it",
        )
    }
}
