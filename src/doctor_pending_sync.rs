//! bl-341b: legacy `pending-sync/` diagnostic. The dedicated
//! `pending_sync_legacy` warning module was retired with this ball;
//! `bl doctor` now surfaces the same information on demand instead
//! of firing on every `bl` invocation. Lives in its own file so
//! `doctor.rs` stays under the project's 300-line cap.

use crate::doctor::Finding;
use std::fs;
use std::path::Path;

/// Build a finding for `<root>/.balls/local/pending-sync/` when the
/// directory exists and contains at least one file. `None` otherwise.
/// The check is layout-agnostic: XDG and stealth clones simply lack
/// the path and fall through to `None`.
pub(crate) fn finding(root: &Path) -> Option<Finding> {
    let dir = root.join(".balls/local/pending-sync");
    let count = count_files(&dir)?;
    if count == 0 {
        return None;
    }
    Some(Finding::flag(
        format!("{count} staged sync reports remain at {}", dir.display()),
        "the human-gate staging surface was deferred (bl-6969); these \
         files will not be applied. Remove the directory manually once \
         you have archived anything you want to keep",
    ))
}

/// Walk the legacy pending-sync tree (flat files + one level of event
/// subdirs — the shape `bl sync --review` wrote before bl-6969) and
/// tally files. `None` when the root is absent so the caller stays
/// silent on clones that never had the directory.
fn count_files(root: &Path) -> Option<usize> {
    let entries = fs::read_dir(root).ok()?;
    let mut total = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            total += 1;
            continue;
        }
        if path.is_dir() {
            if let Ok(inner) = fs::read_dir(&path) {
                total += inner.flatten().filter(|f| f.path().is_file()).count();
            }
        }
    }
    Some(total)
}

#[cfg(test)]
#[path = "doctor_pending_sync_tests.rs"]
mod tests;
