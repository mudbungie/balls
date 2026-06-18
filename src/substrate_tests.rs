//! Tests for §12 bootstrap-on-miss on throwaway repos: [`found_landing`] lays the
//! `balls/config` landing eagerly (seeding its config from the app default-config,
//! §1/§12, with and without the shipped plugin binaries), and [`materialize`] lays
//! the store LAZILY — checking out an existing branch ref (a clone-in) or founding
//! a fresh orphan only when none exists, idempotently (bl-0a23).

use super::*;
use crate::git::run as git;
use crate::hooks::Hooks;
use crate::layout::Xdg;
use crate::DEFAULT_TASKS_BRANCH;
use tempfile::TempDir;

/// The two checkout paths under a fresh tempdir (neither need pre-exist — founding
/// creates the landing repo and adds the store as a linked worktree).
fn paths(tmp: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    (tmp.path().join("config"), tmp.path().join("tasks"))
}

/// An `Xdg` rooted under `tmp` so the embedded default-config materializes in the
/// tempdir, never the real `$XDG_CONFIG_HOME`.
fn xdg(tmp: &TempDir) -> Xdg {
    Xdg::with(tmp.path(), Some(&tmp.path().join("cfg").to_string_lossy()), Some(&tmp.path().join("st").to_string_lossy()))
}

/// A dir holding fake sibling binaries, standing in for the dir `bl` lives in.
fn exe_dir(tmp: &TempDir, names: &[&str]) -> std::path::PathBuf {
    let dir = tmp.path().join("bin");
    fs::create_dir_all(&dir).unwrap();
    for name in names {
        fs::write(dir.join(name), "#!/bin/sh\n").unwrap();
    }
    dir
}

#[test]
fn found_landing_makes_the_config_branch_with_seeded_config_and_no_store() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();

    // The landing is the balls/config branch with a seeded config/.
    assert_eq!(git(&landing, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), LANDING_BRANCH);
    assert!(landing.join("config").join("balls.toml").is_file());
    assert!(landing.join("config").join("plugins.toml").is_file());
    assert!(landing.join(".gitignore").is_file());
    let log = git(&landing, &["log", "--oneline"], None).unwrap();
    assert_eq!(log.lines().count(), 1);
    assert!(log.contains("balls: found"));
    // The STORE is NOT founded eagerly — that is materialize's lazy job (bl-0a23).
    assert!(!store.exists());
    assert!(git(&landing, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{DEFAULT_TASKS_BRANCH}")], None).is_err());
}

#[test]
fn materialize_founds_an_orphan_store_when_the_branch_is_absent() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    materialize(&landing, &store, DEFAULT_TASKS_BRANCH, "tester").unwrap();

    // The store is the balls/tasks branch — a SEPARATE orphan root (no shared
    // history with the landing) with a tracked tasks/ folder.
    assert_eq!(git(&store, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), DEFAULT_TASKS_BRANCH);
    assert!(store.join("tasks").join(".gitkeep").is_file());
    assert_eq!(git(&store, &["log", "--oneline"], None).unwrap().lines().count(), 1);
    // Orphan: no merge-base with the landing's config branch.
    assert!(git(&landing, &["merge-base", LANDING_BRANCH, DEFAULT_TASKS_BRANCH], None).is_err());
}

#[test]
fn materialize_checks_out_an_existing_branch_instead_of_founding() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    // A branch ref already here — what the prime/pre tracker's clone-in leaves
    // (§12). materialize must CHECK IT OUT, not found a divergent fresh orphan.
    let head = git(&landing, &["rev-parse", "HEAD"], None).unwrap().trim().to_string();
    git(&landing, &["branch", DEFAULT_TASKS_BRANCH, &head], None).unwrap();
    materialize(&landing, &store, DEFAULT_TASKS_BRANCH, "tester").unwrap();
    assert_eq!(git(&store, &["rev-parse", "HEAD"], None).unwrap().trim(), head);
}

#[test]
fn materialize_is_idempotent_once_the_store_exists() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    materialize(&landing, &store, DEFAULT_TASKS_BRANCH, "tester").unwrap();
    let before = git(&store, &["rev-parse", "HEAD"], None).unwrap();
    materialize(&landing, &store, DEFAULT_TASKS_BRANCH, "tester").unwrap(); // a re-prime → no-op
    assert_eq!(git(&store, &["rev-parse", "HEAD"], None).unwrap(), before);
}

#[test]
fn materialize_switches_an_existing_store_onto_a_repointed_branch_ref() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    materialize(&landing, &store, DEFAULT_TASKS_BRANCH, "tester").unwrap();
    // A second store branch already has a ref (a clone-in of the repointed
    // name, §12). Repointing `tasks_branch` and re-materializing must bring the
    // EXISTING store checkout onto it — not no-op on "a store dir exists"
    // (bl-eb52: reads and writes kept hitting the old branch).
    let head = git(&store, &["rev-parse", "HEAD"], None).unwrap().trim().to_string();
    git(&landing, &["branch", "balls/other", &head], None).unwrap();
    materialize(&landing, &store, "balls/other", "tester").unwrap();
    assert_eq!(git(&store, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), "balls/other");
    assert_eq!(git(&store, &["rev-parse", "HEAD"], None).unwrap().trim(), head);
}

#[test]
fn materialize_founds_the_repointed_branch_when_no_ref_exists() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    materialize(&landing, &store, DEFAULT_TASKS_BRANCH, "tester").unwrap();
    // Repointed to a name with NO ref anywhere (`bl install --to <new>` /
    // `conf set task-branch <new>` on a once-primed checkout): materialize must
    // CREATE the ref — a fresh orphan — and switch the store onto it, so later
    // pushes have a `src refspec` that resolves (bl-eb52).
    materialize(&landing, &store, "balls/fresh", "tester").unwrap();
    assert_eq!(git(&store, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), "balls/fresh");
    assert!(store.join("tasks").join(".gitkeep").is_file());
    // A fresh orphan root: a single rootless commit (no parent to resolve).
    assert_eq!(git(&store, &["log", "--oneline"], None).unwrap().lines().count(), 1);
    assert!(git(&store, &["rev-parse", "--verify", "HEAD^"], None).is_err());
}

#[test]
fn the_founding_seeds_carry_the_checkout_scoped_trailers() {
    // §5: checkout-scoped seals carry bl-protocol/bl-op/bl-actor — only bl-id
    // (which names a single ball) is absent (bl-1d9b). Both prime seeds — the
    // landing's `balls: found` and the store's `balls: found store` — conform.
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    materialize(&landing, &store, DEFAULT_TASKS_BRANCH, "tester").unwrap();
    for checkout in [&landing, &store] {
        let msg = git(checkout, &["log", "-1", "--format=%B"], None).unwrap();
        let md = crate::message::parse(&msg).unwrap();
        assert_eq!(md["bl-protocol"], ["1"], "{msg}");
        assert_eq!(md["bl-op"], ["prime"], "{msg}");
        assert_eq!(md["bl-actor"], ["tester"], "{msg}");
        assert!(!md.contains_key("bl-id"), "{msg}");
    }
}

#[test]
fn found_landing_without_any_plugin_binary_seeds_an_empty_schedule() {
    let tmp = TempDir::new().unwrap();
    let (landing, _store) = paths(&tmp);
    found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    // The schedule file exists but every default entry pruned (no binaries here).
    let hooks = Hooks::load(&landing).unwrap();
    assert!(hooks.names("prime", "pre").is_empty());
    assert!(!landing.join("config/plugins/bin").exists());
}

#[test]
fn found_landing_with_the_shipped_binaries_keeps_and_binds_them() {
    let tmp = TempDir::new().unwrap();
    let (landing, _store) = paths(&tmp);
    let exe = exe_dir(&tmp, &["bl-tracker", "bl-delivery"]);
    found_landing(&landing, &xdg(&tmp), Some(&exe), "tester").unwrap();

    // The committed schedule keeps both shipped plugins (§6).
    let hooks = Hooks::load(&landing).unwrap();
    assert_eq!(hooks.names("create", "post"), ["bl-tracker"]);
    assert_eq!(hooks.names("close", "post"), ["bl-delivery", "bl-tracker"]);
    // The LOCAL bin/ bindings exist but are gitignored — only the committed text
    // (balls.toml + plugins.toml) is tracked (§2).
    assert!(landing.join("config/plugins/bin/bl-tracker").symlink_metadata().is_ok());
    assert!(landing.join("config/plugins/bin/bl-delivery").symlink_metadata().is_ok());
    let tracked = git(&landing, &["ls-files", "config"], None).unwrap();
    assert!(tracked.contains("config/plugins.toml"));
    assert!(!tracked.contains("bin/tracker"), "bin/ must not be committed");
}
