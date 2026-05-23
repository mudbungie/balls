//! Unique task-id minting. `SHA1(title + timestamp)` truncated to the
//! repo's configured id length, retried with a stepped timestamp on
//! collision. The single home for id generation: both `bl create` and
//! deferred-mode `bl review`'s auto-gate child mint ids through here,
//! so there is no second copy of the retry loop to drift.

use crate::error::{BallError, Result};
use crate::store::{task_lock, LockGuard, Store};
use crate::task::Task;

/// Mint a unique id AND acquire its per-task lock, atomically with
/// respect to other concurrent `bl create` invocations on the same
/// store. Two minters racing on the same title and microsecond can
/// pre-lock the same id (`next_unique_id`'s `task_exists` check is
/// unlocked, so both compute the same hash and both see the id
/// unused). The per-task lock then serializes them — but the second
/// holder, without this re-check, would silently overwrite the
/// first's task JSON with its own. Re-checking `task_exists` *after*
/// the lock is held closes the seam: a colliding loser observes the
/// winner's file on disk and remints onto a fresh timestamp.
pub fn mint_and_lock(store: &Store, title: &str) -> Result<(String, LockGuard)> {
    mint_and_lock_with(store, title, chrono::Utc::now(), MINT_RECHECK_RETRIES)
}

/// `mint_and_lock`'s injected-knob variant: tests drive the
/// lock-recheck loop deterministically with a fixed `start` time and
/// a tunable `retries` cap. Production callers always go through
/// `mint_and_lock` so `Utc::now()` and the retry cap live in exactly
/// one place.
pub(crate) fn mint_and_lock_with(
    store: &Store,
    title: &str,
    mut start: chrono::DateTime<chrono::Utc>,
    retries: usize,
) -> Result<(String, LockGuard)> {
    let id_length = store.load_project_config()?.id_length;
    for _ in 0..retries {
        let id = next_unique_id(title, id_length, start, &|c| store.task_exists(c))?;
        let guard = task_lock(store, &id)?;
        if !store.task_exists(&id) {
            return Ok((id, guard));
        }
        // Lost the lock race: the holder that ran before us just
        // saved this exact id. Drop the guard, bump the clock, and
        // remint onto a fresh timestamp.
        drop(guard);
        start += chrono::Duration::milliseconds(1);
    }
    Err(BallError::Other(
        "could not mint a unique task id after losing the lock race repeatedly".into(),
    ))
}

const MINT_RECHECK_RETRIES: usize = 1000;

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
    use crate::git_test_support::init_repo;
    use crate::task::NewTaskOpts;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn mint_and_lock_remints_when_lock_recheck_sees_winner() {
        // The race that bl-77a9 fixes: two creators pre-pick the same
        // id (`next_unique_id` is unlocked), the per-task lock
        // serializes them, and the loser must NOT overwrite the
        // winner's file. We force the shape deterministically: hold
        // the would-be-winner's lock from the test, let a worker
        // thread reach `task_lock` on the same id and block, save the
        // winner's file, then drop the external lock. The worker's
        // post-lock recheck must observe the file and remint past it.
        let td = tempdir().unwrap();
        init_repo(td.path());
        let store = Arc::new(Store::init(td.path(), false, None).unwrap());

        let start = chrono::Utc::now();
        let id_length = store.load_project_config().unwrap().id_length;
        let contested = Task::generate_id("race", start, id_length);

        let external = task_lock(&store, &contested).unwrap();

        let store_for_thread = store.clone();
        let handle = thread::spawn(move || {
            mint_and_lock_with(&store_for_thread, "race", start, MINT_RECHECK_RETRIES)
                .map(|(id, _g)| id)
        });

        // Let the worker thread reach `task_lock(contested)` and
        // block. The exact delay only affects which path the worker
        // takes through the loop — the assertion that the returned id
        // differs from `contested` is sound either way — but waiting
        // here exercises the post-lock recheck.
        thread::sleep(Duration::from_millis(50));

        let winner = Task::new(NewTaskOpts { title: "race".into(), ..Default::default() }, contested.clone());
        store.save_task(&winner).unwrap();

        drop(external);

        let losers_id = handle.join().unwrap().unwrap();
        assert_ne!(
            losers_id, contested,
            "the loser of the lock race must remint, not overwrite the winner's file"
        );
        assert!(store.task_exists(&contested), "winner's file must survive");
    }

    #[test]
    fn mint_and_lock_with_zero_retries_returns_exhaustion_err() {
        // Cover the post-loop exhaustion branch: a zero retry budget
        // never enters the body and falls straight through to the
        // explicit Err. The shape is the same one a (pathological)
        // continuous lock race would hit in production.
        let td = tempdir().unwrap();
        init_repo(td.path());
        let store = Store::init(td.path(), false, None).unwrap();
        let res = mint_and_lock_with(&store, "x", chrono::Utc::now(), 0);
        let Err(err) = res else {
            panic!("expected Err from zero retry budget");
        };
        match err {
            BallError::Other(s) => assert!(s.contains("lock race")),
            other => panic!("expected Other, got {other:?}"),
        }
    }

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
