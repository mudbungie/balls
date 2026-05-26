//! One-shot migration warning for clones that still carry a
//! `pending-sync/` directory from before bl-6969 deferred the human-gate
//! staging surface. The directory is the operator's record of staged
//! reports a prior `bl sync --review` produced; we never auto-delete it.
//! The warning fires once per process whenever the directory exists with
//! at least one file so the operator knows the files won't replay.

use crate::store::Store;
use std::fs;
use std::sync::OnceLock;

static WARNED: OnceLock<()> = OnceLock::new();

const PENDING_DIR: &str = "pending-sync";

/// Emit the legacy-pending-sync warning at most once per process if the
/// clone still has a populated `pending-sync/` tree.
pub fn warn_if_present(store: &Store) {
    let root = store.local_dir().join(PENDING_DIR);
    let Some(count) = count_files(&root) else { return };
    if count == 0 {
        return;
    }
    WARNED.get_or_init(|| {
        eprintln!(
            "warning: {count} staged sync reports remain at {} from a prior bl. The staging feature is deferred (see bl-6969); these files will not be applied automatically. Re-run the affected plugin syncs after the deferral is resolved.",
            root.display()
        );
    });
}

/// Walk every event subdir and tally files. `None` when the root is
/// absent so the caller can quietly skip the warning.
fn count_files(root: &std::path::Path) -> Option<usize> {
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
#[path = "pending_sync_legacy_tests.rs"]
mod tests;
