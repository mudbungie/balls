//! Tests for §12/§13 `prime`/`sync` orchestration. Chains run plugin-free
//! (`exe_dir: None` ⇒ every default hook prunes ⇒ no subprocess), so these
//! exercise the core logic — bootstrap of both branches, the seed, binding, flag
//! parsing — without a plugin binary; the end-to-end chain is `tests/dispatch.rs`.

use super::*;
use crate::edge::Edge;
use crate::layout::Xdg;
use std::path::PathBuf;
use tempfile::TempDir;

/// An edge rooted in `tmp` with the given (optional) `bl`-sibling dir.
fn edge(tmp: &TempDir, exe_dir: Option<PathBuf>) -> Edge {
    Edge {
        xdg: Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        exe_dir,
        color: false,
        log_level: None,
    }
}

/// The landing checkout this edge resolves to.
fn landing(e: &Edge) -> PathBuf {
    e.xdg.clone_dir(&e.invocation_path).landing()
}

/// The store checkout this edge resolves to.
fn store(e: &Edge) -> PathBuf {
    e.xdg.clone_dir(&e.invocation_path).store()
}

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(ToString::to_string).collect()
}

/// The op-log this edge writes to (core emits a `begin` record per op, §6).
fn op_log(e: &Edge) -> String {
    let path = e.xdg.clone_dir(&e.invocation_path).op_log();
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn prime_founds_both_branches_on_a_miss_then_converges_on_the_hit_path() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--as", "me"])).unwrap();
    assert!(landing(&e).join("config").join("balls.toml").is_file());
    assert!(store(&e).join("tasks").is_dir());
    // Re-prime: both checkouts already exist → hit path (rebind None), no error.
    prime(&e, &[]).unwrap();
}

#[test]
fn prime_drives_a_sync_after_the_prime_chain() {
    // §12/§13 gap (A): prime is an orchestrator of syncs — after the prime chain
    // it must drive `sync` so an established checkout is brought current. Core
    // logs a `begin` per op (§6), so a `sync` op record in the op-log proves the
    // driven sync ran (the chain is tracker-free, so the fetch itself no-ops).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--as", "me"])).unwrap();
    let log = op_log(&e);
    assert!(log.contains("\"op\":\"prime\""), "prime chain ran: {log}");
    assert!(log.contains("\"op\":\"sync\""), "prime drove a sync: {log}");
}

#[test]
fn a_seed_naming_the_landing_as_tasks_branch_fails_prime_named_and_conf_set_recovers() {
    // bl-ac89: `tasks_branch = balls/config` is structurally impossible — one
    // branch cannot back two worktrees of one repo. A poisoned seed used to
    // wedge first prime on a raw git fatal (`already used by worktree`); now the
    // §4 read authority refuses it BY NAME, the landing still founds, and the
    // `conf set task-branch` fix path stays open.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let seed = e.xdg.default_config();
    std::fs::create_dir_all(&seed).unwrap();
    std::fs::write(seed.join("balls.toml"), "tasks_branch = \"balls/config\"\n").unwrap();
    let err = prime(&e, &[]).unwrap_err().to_string();
    assert!(err.contains("names the landing"), "{err}");
    assert!(landing(&e).join("config").is_dir(), "the landing founded before the refusal");
    // Recovery is one conf write, then prime converges normally.
    crate::conf::run(&e, &argv(&["set", "task-branch", "balls/tasks"])).unwrap();
    prime(&e, &[]).unwrap();
    assert!(store(&e).join("tasks").is_dir());
}

#[test]
fn sync_before_prime_is_an_error() {
    let tmp = TempDir::new().unwrap();
    assert!(sync(&edge(&tmp, None), &[]).is_err());
}

#[test]
fn sync_targets_the_store_and_special_cases_no_branch_name() {
    // §13: core keys on NO literal token — the landing's no-op falls out of the
    // tracker's general rule (no upstream ⇒ nothing fetched), so every name,
    // the landing branch included, takes the one general path through the chain.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    sync(&e, &[]).unwrap(); // no arg: sync the store
    sync(&e, &argv(&["work/bl-1234", "--as", "me"])).unwrap(); // a named target
    sync(&e, &argv(&[crate::LANDING_BRANCH])).unwrap(); // the landing, by its real name
    let (l, s) = (landing(&e), store(&e));
    let (b, _) = bind(&e, &l, &s, None, Some(crate::LANDING_BRANCH.into()), false).unwrap();
    assert_eq!(b.tasks_branch, crate::LANDING_BRANCH); // rides the binding untouched
}

#[test]
fn a_named_sync_branch_overrides_the_config_tasks_branch_in_the_binding() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    let (l, s) = (landing(&e), store(&e));
    // No target ⇒ the config-named store branch; a target ⇒ that branch, which
    // is the one datum the tracker fetches/ff's (§13 `bl sync <branch>`).
    let (default_b, _) = bind(&e, &l, &s, None, None, false).unwrap();
    let (named_b, _) = bind(&e, &l, &s, None, Some("federation/shared".into()), false).unwrap();
    assert_eq!(named_b.tasks_branch, "federation/shared");
    assert_ne!(default_b.tasks_branch, named_b.tasks_branch);
}

#[test]
fn prime_rejects_unknown_flags_and_a_missing_value() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    assert!(prime(&e, &argv(&["--bogus"])).is_err());
    assert!(prime(&e, &argv(&["--as"])).is_err()); // flag with no value
    assert!(prime(&e, &argv(&["--remote"])).is_err()); // override flag with no value
    assert!(prime(&e, &argv(&["--center"])).is_err());
    assert!(prime(&e, &argv(&["--install"])).is_err()); // adopt flag with no value
}

#[test]
fn prime_accepts_the_remote_override_flags() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    // --remote and --center both name the store remote; the empty (tracker-free)
    // chain ignores it, so this just proves they parse and resolve into the binding.
    prime(&e, &argv(&["--remote", "git@hub:r"])).unwrap();
    prime(&e, &argv(&["--center", "git@hub:c", "--remote", "git@hub:r"])).unwrap();
}

#[test]
fn prime_rejects_stealth_combined_with_any_remote_naming_flag() {
    // §12: --stealth opts out of any store remote, so a flag that NAMES one
    // contradicts it — refused loud at parse, never silently picking a winner.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    for contradictory in [
        ["--stealth", "--remote", "git@hub:r"],
        ["--stealth", "--center", "git@hub:c"],
        ["--install", "git@hub:c", "--stealth"],
    ] {
        let err = prime(&e, &argv(&contradictory)).unwrap_err().to_string();
        assert!(err.contains("--stealth contradicts"), "{err}");
    }
}

#[test]
fn a_stealth_prime_binds_no_remote_outranking_even_the_xdg_one() {
    // §12 `bl prime --stealth`: the binding carries `stealth` and NO remote.
    // The CLI layer outranks config (§4), so the per-machine XDG `remote` —
    // the one remote tier the parse cannot forbid — is dropped too.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--stealth", "--as", "me"])).unwrap(); // the full verb runs
    let user_config = e.xdg.user_config();
    std::fs::create_dir_all(user_config.parent().unwrap()).unwrap();
    std::fs::write(&user_config, "remote = \"git@hub:r\"\n").unwrap();
    let (l, s) = (landing(&e), store(&e));
    let (tracked, _) = bind(&e, &l, &s, None, None, false).unwrap();
    assert_eq!(tracked.remote.as_deref(), Some("git@hub:r")); // XDG tier resolves
    assert!(!tracked.stealth);
    let (stealth, _) = bind(&e, &l, &s, None, None, true).unwrap();
    assert_eq!(stealth.remote, None); // --stealth drops it
    assert!(stealth.stealth);
}

#[test]
fn sync_rejects_unknown_flags_and_a_second_branch() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    assert!(sync(&e, &argv(&["--bogus"])).is_err());
    assert!(sync(&e, &argv(&["-x"])).is_err()); // single-dash unknown is a flag, not a branch
    assert!(sync(&e, &argv(&["a", "b"])).is_err());
}

#[test]
fn sync_accepts_the_per_op_remote_override() {
    // The ONE §12 ladder (bl-c2de): sync takes `--remote`/`--center` exactly as
    // prime does; the plugin-less chain ignores it, so this proves parse+bind.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    sync(&e, &argv(&["--remote", "git@hub:r"])).unwrap();
    sync(&e, &argv(&["--center", "git@hub:c", "--remote", "git@hub:r"])).unwrap();
    assert!(sync(&e, &argv(&["--remote"])).is_err()); // flag with no value
}
