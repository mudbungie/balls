//! `bl init --bare` — XDG-aware bare-clone bootstrap (bl-be70).
//!
//! Phase 1B-7 flipped `--bare` to write the XDG layout: a bare gitdir
//! at `<clone>/.git` plus an XDG tracker checkout under
//! `~/.local/state/balls/trackers/<enc-source>/balls%2Ftasks/` and the
//! per-clone catch-all dirs under the bare clone's nested path.
//! Nothing under the bare clone's working tree (there is no working
//! tree) is balls-shaped.

mod common;

use common::*;
use predicates::prelude::*;

/// Stand up a published XDG project on a bare remote, with one task
/// already filed. Returns the remote, the source URL, and the task
/// title for assertion.
fn published_remote() -> (Repo, String, String) {
    let xdg = new_xdg_repo();
    let title = "clone task".to_string();
    let _id = create_task(xdg.clone.path(), &title);
    // The donor's `bl create` already pushed `balls/tasks` to origin via
    // sync; nothing else to publish for the bare clone to pick up.
    let source_url = xdg.remote.path().to_string_lossy().into_owned();
    (xdg.remote, source_url, title)
}

#[test]
fn bare_clone_bootstrap_materializes_xdg_layout() {
    let (_remote, source_url, title) = published_remote();
    let run = tmp();
    let clone = run.path().join("proj-clone");

    bl(run.path())
        .args(["init", "--bare"])
        .arg(&source_url)
        .arg(&clone)
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized bare balls clone"));

    // Bare gitdir at <clone>/.git — the working-tree side has no
    // balls footprint (no .balls/, no .balls-worktrees/).
    assert_eq!(
        git(&clone, &["rev-parse", "--is-bare-repository"]).trim(),
        "true"
    );
    assert!(!clone.join(".balls").exists(), "no legacy .balls/ under bare root");
    assert!(
        !clone.join(".balls-worktrees").exists(),
        "no legacy worktrees dir under bare root"
    );

    // XDG tracker checkout is shared with the donor (same origin URL ⇒
    // same `<enc-origin>`); the source had one task, so the bare
    // clone resolves it directly through the same checkout.
    let tracker = discover_state_repo(&clone).expect("xdg tracker resolves");
    assert!(tracker.join(".git").exists(), "tracker checkout materialized");
    assert!(
        tracker.join(".balls/tasks").exists(),
        "tracker carries the state branch's tasks/"
    );

    // Per-clone XDG dirs are keyed on the bare clone's nested path, so
    // the bare clone gets its own claims/locks/plugins-auth (disjoint
    // from the donor).
    assert!(claims_dir(&clone).is_dir());
    assert!(lock_dir(&clone).is_dir());
    assert!(plugins_auth_dir(&clone).is_dir());

    // The clone serves the project's tasks from the bare root.
    bl(&clone)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_clone_bootstrap_clones_tracker_when_not_yet_materialized() {
    // A published bare remote with `balls/tasks` but no donor XDG
    // tracker checkout on this host — exercises the "fresh clone"
    // path through `materialize_tracker_from_source`. The other tests
    // use `published_remote()` which materializes the tracker checkout
    // as a side effect of the donor's `Store::init_xdg`, hitting the
    // warm path instead.
    let tracker = common::tracker::new_tracker();
    let run = tmp();
    let clone = run.path().join("proj-clone");

    bl(run.path())
        .args(["init", "--bare"])
        .arg(tracker.path())
        .arg(&clone)
        .assert()
        .success();

    // The tracker checkout for this `<enc-source>` is freshly cloned
    // (not pre-materialized by any donor on this host).
    let xdg = discover_state_repo(&clone).expect("xdg tracker resolves");
    assert!(xdg.join(".git").exists(), "tracker checkout materialized");
    assert!(
        xdg.join(".balls/tasks").exists(),
        "fresh tracker carries the state branch's tasks/"
    );
}

#[test]
fn bare_clone_bootstrap_is_idempotent() {
    let (_remote, source_url, title) = published_remote();
    let run = tmp();
    let clone = run.path().join("proj-clone");

    for _ in 0..2 {
        bl(run.path())
            .args(["init", "--bare"])
            .arg(&source_url)
            .arg(&clone)
            .assert()
            .success();
    }
    // Second run reused the bare gitdir and the warm tracker checkout;
    // the store still resolves and lists the task.
    bl(&clone)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_clone_rejects_stealth_or_tasks_dir_combo() {
    let run = tmp();
    let clone = run.path().join("proj-clone");
    bl(run.path())
        .args(["init", "--bare"])
        .arg("/some/source")
        .arg(&clone)
        .arg("--stealth")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--bare cannot be combined with --stealth or --tasks-dir",
        ));
}

#[test]
fn bare_clone_source_without_balls_errors_clearly() {
    // A remote whose `main` was never `bl init`ed: no `balls/tasks`
    // branch to clone into the tracker checkout.
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "bob");
    std::fs::write(dev.path().join("README"), "hi\n").unwrap();
    git(dev.path(), &["add", "-A"]);
    git(dev.path(), &["commit", "-qm", "init", "--no-verify"]);
    git(dev.path(), &["push", "-q", "origin", "main"]);

    let run = tmp();
    let clone = run.path().join("proj-clone");
    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&clone)
        .assert()
        .failure()
        .stderr(predicate::str::contains("balls/tasks"));
}

#[test]
fn bare_clone_refuses_to_clobber_a_non_bare_gitdir() {
    let (_remote, source_url, _title) = published_remote();
    let run = tmp();
    let clone = run.path().join("proj-clone");
    // Pre-existing *non-bare* .git at the clone path: must be refused,
    // not overwritten (non-destructive, like working-tree `bl init`).
    std::fs::create_dir_all(&clone).unwrap();
    git(&clone, &["init", "-q", "-b", "main"]);
    assert!(clone.join(".git").is_dir());

    bl(run.path())
        .args(["init", "--bare"])
        .arg(&source_url)
        .arg(&clone)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a bare repo"));
}
