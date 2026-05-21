//! `doctor`'s `.balls/tasks` convenience-symlink probe. Lifted out of
//! `doctor.rs` for line-budget. Read-only: flags drift, hints at the
//! recreate path. Re-materialization itself belongs to
//! `state_repo::ensure`, which `bl prime` re-runs.

use crate::doctor::Finding;
use std::fs;
use std::path::Path;

/// Validate `<root>/.balls/tasks`. Expected: a symlink whose
/// `read_link` target equals `expected` (a path relative to `.balls/`).
/// Surfaces missing, stray non-symlink, and stale target. Doctor never
/// mutates — every fix hint goes through `bl prime`, which re-runs
/// `state_repo::ensure` and re-materializes the symlink.
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
                "remove {} and re-run `bl prime` to repoint it",
                link.display()
            ),
        ));
    }
}

fn missing_or_stray(link: &Path) -> Finding {
    if link.exists() {
        Finding::flag(
            format!("{} is not a symlink (stray file or directory)", link.display()),
            format!("remove {} and re-run `bl prime`", link.display()),
        )
    } else {
        Finding::flag(
            format!("{} convenience symlink is missing", link.display()),
            "re-run `bl prime` to recreate it",
        )
    }
}
