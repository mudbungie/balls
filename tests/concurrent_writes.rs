//! Parallelism regression: multiple workers hitting the state
//! branch concurrently on DIFFERENT tasks must all succeed. Guards
//! against the git `index.lock` race that used to happen when
//! `commit_task` / `commit_staged` / `remove_task` had no store-wide
//! serialization.

mod common;

use common::*;
use std::thread;

const PARALLEL_WORKERS: usize = 8;

#[test]
fn parallel_bl_create_across_workers_all_succeed() {
    let repo = new_repo();
    init_in(repo.path());
    let root = repo.path().to_path_buf();

    let handles: Vec<_> = (0..PARALLEL_WORKERS)
        .map(|i| {
            let root = root.clone();
            thread::spawn(move || {
                let out = bl(&root)
                    .args(["create", &format!("parallel-{}", i)])
                    .output()
                    .expect("bl create");
                (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
            })
        })
        .collect();

    let mut successes = 0;
    let mut failures = Vec::new();
    for h in handles {
        let (ok, stderr) = h.join().unwrap();
        if ok {
            successes += 1;
        } else {
            failures.push(stderr);
        }
    }
    assert_eq!(
        successes, PARALLEL_WORKERS,
        "expected all {} creates to succeed, failures: {:#?}",
        PARALLEL_WORKERS, failures
    );

    // All tasks visible after the storm settles.
    let list_out = bl(&root).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&list_out.stdout);
    for i in 0..PARALLEL_WORKERS {
        assert!(
            s.contains(&format!("parallel-{}", i)),
            "list missing parallel-{}: {}",
            i,
            s
        );
    }
}

#[test]
fn parallel_bl_create_and_update_mix() {
    // A mix of creates and updates running concurrently against the
    // state worktree — exercises the lock under commit_task's
    // regular (non-close) path for both operation flavors.
    let repo = new_repo();
    init_in(repo.path());
    let root = repo.path().to_path_buf();

    // Seed a task we can update from other threads.
    let seed_id = create_task(repo.path(), "seed");

    let mut handles = Vec::new();
    for i in 0..PARALLEL_WORKERS / 2 {
        let root = root.clone();
        handles.push(thread::spawn(move || {
            let out = bl(&root)
                .args(["create", &format!("mix-create-{}", i)])
                .output()
                .expect("bl create");
            out.status.success()
        }));
    }
    for i in 0..PARALLEL_WORKERS / 2 {
        let root = root.clone();
        let id = seed_id.clone();
        handles.push(thread::spawn(move || {
            let out = bl(&root)
                .args(["update", &id, "--note", &format!("worker {} note", i)])
                .output()
                .expect("bl update");
            out.status.success()
        }));
    }

    for h in handles {
        assert!(h.join().unwrap(), "a parallel worker failed");
    }
}
