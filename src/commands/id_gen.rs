//! Task id generation with collision retry.

use balls::error::{BallError, Result};
use balls::store::Store;
use balls::task::Task;

/// Generate a task id that does not yet exist in the store, retrying with an
/// incremented timestamp on collision. Returns an error if no unique id can
/// be found within a reasonable number of attempts.
pub(crate) fn generate_unique_id(title: &str, store: &Store, id_length: usize) -> Result<String> {
    next_unique_id(title, id_length, chrono::Utc::now(), |id| {
        store.task_exists(id)
    })
}

/// Pure form of the retry loop: ask `exists` whether each candidate is taken,
/// stepping the timestamp forward on collision. Split out so the retry and
/// exhaustion paths can be tested deterministically without standing up a
/// real Store.
fn next_unique_id<F>(
    title: &str,
    id_length: usize,
    start: chrono::DateTime<chrono::Utc>,
    exists: F,
) -> Result<String>
where
    F: Fn(&str) -> bool,
{
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
        let id = next_unique_id("hello", 4, now, |_| false).unwrap();
        assert_eq!(id, Task::generate_id("hello", now, 4));
    }

    #[test]
    fn retries_past_collision() {
        let now = chrono::Utc::now();
        let first = Task::generate_id("hello", now, 4);
        let mut taken = HashSet::new();
        taken.insert(first.clone());
        let id = next_unique_id("hello", 4, now, |c| taken.contains(c)).unwrap();
        assert_ne!(id, first);
    }

    #[test]
    fn exhausts_after_1000_tries() {
        let calls = RefCell::new(0usize);
        let err = next_unique_id("x", 4, chrono::Utc::now(), |_| {
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
