//! Shared fixtures for the read-verb tests: build a [`Catalog`] from in-memory
//! balls without a git checkout. Each ball is written to a throwaway tempdir and
//! reloaded, so the tests exercise the real `tasks/` parse path.

use tempfile::TempDir;

use super::Catalog;
use crate::task::{Blocker, On, Task};

/// A minimal ready ball: a title and a timestamp, everything else default.
pub(crate) fn task(title: &str, created: i64) -> Task {
    Task { title: title.into(), created, updated: created, ..Default::default() }
}

/// A `{id, on}` blocker edge.
pub(crate) fn blocker(id: &str, on: On) -> Blocker {
    Blocker { id: id.into(), on }
}

/// Write each `(id, task)` to a fresh store tempdir and load the catalog. The
/// tempdir may drop after — [`Catalog::load`] reads every file into memory.
pub(crate) fn catalog(tasks: &[(&str, Task)]) -> Catalog {
    let tmp = TempDir::new().unwrap();
    for (id, t) in tasks {
        crate::taskfile::write_task(tmp.path(), id, t).unwrap();
    }
    Catalog::load(tmp.path()).unwrap()
}
