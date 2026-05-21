//! Unique task-id minting. `SHA1(title + timestamp)` truncated to the
//! repo's configured id length, retried with a stepped timestamp on
//! collision. The single home for id generation: both `bl create` and
//! deferred-mode `bl review`'s auto-gate child mint ids through here,
//! so there is no second copy of the retry loop to drift.

use crate::error::{BallError, Result};
use crate::store::Store;
use crate::task::Task;

/// Generate an id not already present in `store`, reading the id
/// length from the project config. Thin store-aware wrapper over the
/// pure `next_unique_id` loop.
pub fn generate_task_id(store: &Store, title: &str) -> Result<String> {
    let id_length = store.load_project_config()?.id_length;
    next_unique_id(title, id_length, chrono::Utc::now(), &|id| {
        store.task_exists(id)
    })
}

/// Pure form of the retry loop: ask `exists` whether each candidate is
/// taken, stepping the timestamp forward on collision. Split out so the
/// retry and exhaustion paths are testable without a real Store, and
/// shared with `remaster`'s clash-renaming. `exists` is `&dyn Fn` (not
/// a generic) so the loop is a single monomorphization — every branch's
/// coverage lands on one instance.
pub(crate) fn next_unique_id(
    title: &str,
    id_length: usize,
    start: chrono::DateTime<chrono::Utc>,
    exists: &dyn Fn(&str) -> bool,
) -> Result<String> {
    let mut now = start;
    let mut id = Task::generate_id(title, now, id_length);
    let mut tries = 0;
    while exists(&id) {
        tries += 1;
        if tries > 1000 {
            return Err(BallError::Other(
                "could not generate unique task id after 1000 tries".into(),
            ));
        }
        now += chrono::Duration::milliseconds(1);
        id = Task::generate_id(title, now, id_length);
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[test]
    fn returns_first_id_when_unused() {
        let now = chrono::Utc::now();
        let id = next_unique_id("hello", 4, now, &|_| false).unwrap();
        assert_eq!(id, Task::generate_id("hello", now, 4));
    }

    #[test]
    fn retries_past_collision() {
        let now = chrono::Utc::now();
        let first = Task::generate_id("hello", now, 4);
        let mut taken = HashSet::new();
        taken.insert(first.clone());
        let id = next_unique_id("hello", 4, now, &|c| taken.contains(c)).unwrap();
        assert_ne!(id, first);
    }

    #[test]
    fn exhausts_after_1000_tries() {
        let calls = RefCell::new(0usize);
        let err = next_unique_id("x", 4, chrono::Utc::now(), &|_| {
            *calls.borrow_mut() += 1;
            true
        })
        .unwrap_err();
        match err {
            BallError::Other(s) => assert!(s.contains("1000 tries")),
            other => panic!("expected Other, got {other:?}"),
        }
        assert!(*calls.borrow() > 1000);
    }
}
