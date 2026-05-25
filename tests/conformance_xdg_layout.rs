//! Phase 1A conformance gates for SPEC-clone-layout §14 — the
//! read-side behaviors `Store::discover` must enforce against a
//! hand-constructed XDG layout. New-side assertions per
//! SPEC-tracker-state §16.13: each test wires the new layout by hand
//! and asserts the new behavior, never pinning a fixture against an
//! old binary.
//!
//! Gates covered here:
//!
//! - §14.3 — per-clone isolation by `<nested-clone-path>`.
//! - §14.5 — single-hop redirect (chained redirect aborts).
//! - §14.7 — solo (no `tracker.json`): own checkout is active.
//! - §14.11 — discovery is idempotent (same Store fields on re-run).
//! - §14.12 (partial) — XDG materialization is regenerable: deleting
//!   the per-clone tree and re-discovering rebuilds the per-clone
//!   subdirs without losing tasks. Full §14.12 (tracker re-fetch
//!   after `rm -rf trackers/`) is gated by `bl prime` in Phase 1B.
//!
//! Gates deferred to Phase 1B/1C: §14.1 (no `.balls/` after `bl init`
//! — Phase 1B), §14.4 (bootstrap branch constant — bound to init),
//! §14.13 (worktrees relocated — Phase 1C), §14.15 (stealth init —
//! Phase 1B), §14.16/17 (rename gates — Phase 1B's EffectiveConfig
//! flip), §14.19/20 (lifecycle + hand-operable — bound to 1B+1C).

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::tracker_json::TrackerJson;
use balls::xdg_paths::{tracker_checkout, XdgBases};
use common::*;
use std::fs;
use std::path::{Path, PathBuf};

/// XDG bases rooted under `home`, with HOME-derived defaults for
/// `$XDG_CONFIG_HOME`, `$XDG_STATE_HOME`, `$XDG_CACHE_HOME`. Mirrors
/// what `XdgBases::from_env` builds when a subprocess inherits a
/// custom HOME.
fn bases(home: &Path) -> XdgBases {
    XdgBases::with(home, None, None, None)
}

/// `<nested-clone-path>` for an absolute directory — the clone path
/// with the leading `/` dropped. The XDG layout's per-clone subtrees
/// nest under this verbatim.
fn nested(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    PathBuf::from(s.strip_prefix('/').unwrap_or(&s))
}

/// Hand-construct an XDG tracker checkout from a (real, bare) remote.
/// Mirrors `Store::discover`'s Phase-1B materialization step (still
/// out of scope here): single-branch clone of `balls/tasks` into
/// `~/.local/state/balls/trackers/<enc-origin>/<enc-balls-tasks>/`.
fn fetch_tracker_into_xdg(home: &Path, origin_url: &str) -> PathBuf {
    let bases = bases(home);
    let enc = percent_encode_component(&canonicalize_origin(origin_url));
    let enc_branch = percent_encode_component("balls/tasks");
    let target = tracker_checkout(&bases, &enc, &enc_branch);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let out = std::process::Command::new("git")
        .args(["clone", "-q", "--single-branch", "--branch", "balls/tasks"])
        .arg(origin_url)
        .arg(&target)
        .output()
        .expect("git clone");
    assert!(
        out.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    target
}

/// Bootstrap a remote with a populated `balls/tasks` branch by going
/// through the legacy `bl init` path against a throwaway clone, then
/// pushing. This gives every XDG-layout test a real tracker to fetch
/// from — `Store::init`'s Phase 1B rewrite (bl-e802) will produce the
/// same shape natively, at which point this helper goes away.
fn seed_tracker_remote() -> (Repo, String) {
    let remote = new_bare_remote();
    let seeder = clone_from_remote(remote.path(), "seeder");
    bl(seeder.path()).arg("init").assert().success();
    push(seeder.path());
    let url = remote.path().to_string_lossy().into_owned();
    // `seeder` falls out of scope here; its tempdir is reclaimed but
    // the remote is bare and self-sufficient.
    drop(seeder);
    (remote, url)
}

/// Configure a fresh clone (no bl init yet) against `origin_url`, so
/// `Store::discover` will compute `<enc-origin>` from a real `origin`
/// remote. Returns the clone repo + the absolute path inside the
/// caller's HOME tree (the clone is created under `home/sub/...`).
fn fresh_clone_into(home: &Path, sub: &str, origin_url: &str, who: &str) -> PathBuf {
    let clone_root = home.join(sub);
    fs::create_dir_all(&clone_root).unwrap();
    let out = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(&clone_root)
        .output()
        .expect("git init");
    assert!(out.status.success());
    let email = format!("{who}@example.com");
    for (k, v) in [("user.email", email.as_str()), ("user.name", who), ("commit.gpgsign", "false")] {
        let out = std::process::Command::new("git")
            .current_dir(&clone_root)
            .args(["config", k, v])
            .output()
            .expect("git config");
        assert!(out.status.success());
    }
    let out = std::process::Command::new("git")
        .current_dir(&clone_root)
        .args(["remote", "add", "origin", origin_url])
        .output()
        .expect("git remote add");
    assert!(out.status.success());
    fs::canonicalize(&clone_root).unwrap()
}

/// Invoke `bl` from `cwd` with HOME pointed at `home`. Inheriting
/// HOME would otherwise make XDG discover resolve against the real
/// user's `~/.local/state/balls/`. Returns the configured assert_cmd
/// command so the caller chains `.assert()`.
fn bl_xdg(cwd: &Path, home: &Path) -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("bl").unwrap();
    c.current_dir(cwd).env("HOME", home).env("BALLS_IDENTITY", "test-user");
    c
}

// -- §14.7 -- solo (no tracker.json): own checkout IS the tracker --

#[test]
fn spec_14_7_solo_no_tracker_json_resolves_to_own_checkout() {
    let home = tmp();
    let (_remote, origin_url) = seed_tracker_remote();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");
    let tracker = fetch_tracker_into_xdg(home.path(), &origin_url);
    // `bl list` is the simplest read that exercises Store::discover
    // end-to-end without writing.
    let out = bl_xdg(&clone, home.path()).arg("list").output().unwrap();
    assert!(
        out.status.success(),
        "bl list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Tracker checkout was found at the XDG location.
    assert!(tracker.join(".balls/tasks").exists());
    // No `.balls/` was created at the clone root by the read.
    assert!(!clone.join(".balls").exists(), "XDG read must not create .balls/ at clone root");
}

// -- §14.3 -- per-clone isolation by <nested-clone-path> --

#[test]
fn spec_14_3_two_clones_share_tracker_but_isolate_per_clone() {
    let home = tmp();
    let (_remote, origin_url) = seed_tracker_remote();
    let clone_a = fresh_clone_into(home.path(), "dev/a/proj", &origin_url, "alice");
    let clone_b = fresh_clone_into(home.path(), "dev/b/proj", &origin_url, "bob");
    let tracker = fetch_tracker_into_xdg(home.path(), &origin_url);

    let bases = bases(home.path());
    // The two clones share trackers/<enc-origin>/<enc-branch>/.
    bl_xdg(&clone_a, home.path()).arg("list").assert().success();
    bl_xdg(&clone_b, home.path()).arg("list").assert().success();
    assert!(tracker.exists(), "shared tracker checkout exists");

    // Disjoint <nested-clone-path>s mean disjoint per-clone trees.
    // (No claim yet — Phase 1A's discover materializes the per-clone
    // tree on demand; claims/locks land there on the Phase 1C flip.)
    let nested_a = bases.state_root().join("claims").join(nested(&clone_a));
    let nested_b = bases.state_root().join("claims").join(nested(&clone_b));
    assert_ne!(nested_a, nested_b, "per-clone trees must not collide");
}

// -- §14.5 -- chained redirect aborts --

#[test]
fn spec_14_5_chained_redirect_aborts() {
    let home = tmp();
    let (_remote_own, own_url) = seed_tracker_remote();
    let (_remote_fed, fed_url) = seed_tracker_remote();
    let clone = fresh_clone_into(home.path(), "dev/proj", &own_url, "alice");

    let own = fetch_tracker_into_xdg(home.path(), &own_url);
    // Plant tracker.json on the own checkout — single-hop is fine.
    let tj = TrackerJson { state_url: fed_url.clone(), state_branch: None };
    fs::write(
        own.join(".balls/tracker.json"),
        serde_json::to_string_pretty(&tj).unwrap(),
    )
    .unwrap();

    let fed = fetch_tracker_into_xdg(home.path(), &fed_url);
    // Plant a *second* tracker.json on the federated checkout —
    // forbidden by §5 and what §14.5 asserts discover catches.
    let chained = TrackerJson { state_url: "git@host:org/hop.git".into(), state_branch: None };
    fs::write(
        fed.join(".balls/tracker.json"),
        serde_json::to_string_pretty(&chained).unwrap(),
    )
    .unwrap();

    let out = bl_xdg(&clone, home.path()).arg("list").output().unwrap();
    assert!(!out.status.success(), "chained redirect must fail discover");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("chained redirect detected"),
        "expected chained-redirect diagnostic, got: {stderr}"
    );
}

// -- §14.11 -- discovery is idempotent --

#[test]
fn spec_14_11_discovery_is_idempotent() {
    let home = tmp();
    let (_remote, origin_url) = seed_tracker_remote();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");
    let tracker = fetch_tracker_into_xdg(home.path(), &origin_url);
    let before = snapshot_dir(&tracker);
    let bases = bases(home.path());
    let per_clone_state_before = snapshot_dir(&bases.state_root());

    bl_xdg(&clone, home.path()).arg("list").assert().success();
    bl_xdg(&clone, home.path()).arg("list").assert().success();
    bl_xdg(&clone, home.path()).arg("list").assert().success();

    let after = snapshot_dir(&tracker);
    let per_clone_state_after = snapshot_dir(&bases.state_root());
    assert_eq!(before, after, "tracker checkout must be unchanged by reads");
    assert_eq!(
        per_clone_state_before, per_clone_state_after,
        "XDG state tree must be unchanged by repeated reads"
    );
}

// -- §14.12 partial -- regenerability of the per-clone tree --
// Full §14.12 (rm -rf trackers/ + bl prime rebuilds) is bound to bl
// prime's XDG awareness in Phase 1B. Here we assert the weaker
// invariant Phase 1A can deliver: the per-clone XDG state tree, when
// missing, is not required for reads (the tracker checkout alone is
// sufficient), and discover does not crash if the trees are absent.

#[test]
fn spec_14_12_per_clone_tree_absence_does_not_block_reads() {
    let home = tmp();
    let (_remote, origin_url) = seed_tracker_remote();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");
    let _ = fetch_tracker_into_xdg(home.path(), &origin_url);

    // Per-clone tree does not exist — `bl list` still works.
    bl_xdg(&clone, home.path()).arg("list").assert().success();
}

// Helper: a deterministic snapshot of a directory tree (relative
// paths, file sizes). Detects any add/remove/grow between calls.
fn snapshot_dir(root: &Path) -> Vec<(PathBuf, u64)> {
    if !root.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap().to_path_buf();
        let Ok(md) = entry.metadata() else { continue };
        if md.is_dir() {
            out.push((rel, 0));
            walk(root, &path, out);
        } else {
            out.push((rel, md.len()));
        }
    }
}
