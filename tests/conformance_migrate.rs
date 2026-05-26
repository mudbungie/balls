//! Phase 2 conformance gates for SPEC-clone-layout §14 — the
//! `bl migrate` behaviors. New-side assertions per SPEC-tracker-state
//! §16.13: each test exercises the new command end-to-end and asserts
//! the documented post-state, never pinning a fixture against an old
//! binary.
//!
//! Gates covered here:
//!
//! - §14.21 — pre-XDG migration is one-shot and idempotent. The
//!   migration commit on `main` is the single `balls: migrate to XDG
//!   layout` and the in-repo `.balls/` is gone afterwards.
//! - §14.23 — post-migration the pre-XDG fallback never reappears:
//!   re-running `bl migrate` is a no-op, and the migrated clone's
//!   subsequent reads never recreate `.balls/` at the clone root.
//!
//! Per-aspect siblings live in `conformance_migrate_dirty.rs`
//! (refuses-if-dirty + tracker.json federated case) and
//! `conformance_migrate_renames.rs` (post-migration field renames).
//! Helpers shared across all three live in `common::migrate`.
//!
//! Gates intentionally deferred: §14.22 (hashed-XDG migration —
//! the bl-fc50 layout was reverted before any release shipped, no
//! real clone has it; defense-in-depth left for a future ball);
//! §14.19 (no on-main writes across a full lifecycle — blocked on
//! Phase 1B's `bl init` XDG flip and Phase 1C's `bl claim` flip).

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::xdg_paths::{own_tracker_checkout, PerClonePaths};
use common::migrate::{bases, bl_xdg, legacy_clone, nested};
use common::*;

// ---- §14.21 — pre-XDG migration is one-shot and idempotent ----

#[test]
fn spec_14_21_pre_xdg_migration_is_one_shot() {
    let home = tmp();
    let (_remote, clone, url) = legacy_clone(home.path(), "dev/proj");

    // Pre-state assertions: pre-XDG layout in place.
    assert!(clone.join(".balls/config.json").exists());
    assert!(clone.join(".balls/state-repo/.git").exists());

    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();

    // (a) Documented XDG tree is materialized.
    let enc_origin = percent_encode_component(&canonicalize_origin(&url));
    let tracker = own_tracker_checkout(&bases(home.path()), &enc_origin);
    assert!(tracker.join(".git").exists(), "XDG tracker checkout missing");
    assert!(tracker.join(".balls/repo.json").exists(), "repo.json not written");
    // Solo clone: no tracker.json redirect.
    assert!(
        !tracker.join(".balls/tracker.json").exists(),
        "solo clone must not carry tracker.json"
    );

    // (b) Per-clone XDG tree exists.
    let per_clone = PerClonePaths::new(&bases(home.path()), &nested(&clone));
    assert!(per_clone.claims.exists());
    assert!(per_clone.locks.exists());
    assert!(per_clone.plugins_auth.exists());

    // (c) On-main cleanup landed: no .balls/, no .balls-worktrees/,
    // exactly one migration commit at HEAD.
    assert!(!clone.join(".balls").exists(), ".balls/ must be gone");
    assert!(!clone.join(".balls-worktrees").exists());
    let head = git(&clone, &["log", "-1", "--format=%s"]);
    assert!(
        head.contains("balls: migrate to XDG layout"),
        "head commit should be the migration commit; got {head:?}"
    );
}

#[test]
fn spec_14_21_migration_is_idempotent() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");

    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();
    let head_after_first = git(&clone, &["rev-parse", "HEAD"]).trim().to_string();

    let out = bl_xdg(&clone, home.path()).arg("migrate").output().unwrap();
    assert!(
        out.status.success(),
        "re-run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("nothing to migrate"),
        "re-run must report no-op; got {stdout:?}"
    );
    let head_after_second = git(&clone, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        head_after_first, head_after_second,
        "idempotent re-run must not add a commit"
    );
}

// ---- §14.23 — post-migration fallback never reappears ----

#[test]
fn spec_14_23_post_migration_reads_never_recreate_legacy() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();

    // A subsequent read must not bring `.balls/` back.
    bl_xdg(&clone, home.path()).arg("list").assert().success();
    assert!(
        !clone.join(".balls").exists(),
        "post-migration read must not recreate .balls/ at clone root"
    );
    assert!(
        !clone.join(".balls-worktrees").exists(),
        "post-migration read must not recreate .balls-worktrees/"
    );
}
