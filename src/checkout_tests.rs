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
        path_dirs: Vec::new(),
        color: false,
        log_level: None,
        balls_clock: None,
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
fn prime_founds_over_a_crashed_foundings_config_dir_instead_of_bricking() {
    // bl-ffbf: the §12 founding predicate is a COMMIT on the landing branch, not
    // the `config/` directory founding creates on its way there. A crash in that
    // window used to read as "founded" forever — prime would skip founding, and
    // every op after it opened a change worktree on an unborn HEAD. Keyed on the
    // commit, the debris is just an unfounded landing: prime founds over it and
    // the checkout is ordinary afterwards (a re-prime converges as always).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let l = landing(&e);
    std::fs::create_dir_all(l.join("config")).unwrap();
    crate::git::run(&l, &["init", "-q", "-b", crate::LANDING_BRANCH], None).unwrap();
    assert!(!crate::substrate::is_landing(&l), "a config/ dir with no commit is not a founded landing");

    prime(&e, &argv(&["--as", "me"])).unwrap();

    assert!(crate::substrate::is_landing(&l));
    assert!(l.join("config").join("plugins.toml").is_file());
    assert!(store(&e).join("tasks").is_dir()); // the store materialized off a born HEAD
    prime(&e, &[]).unwrap(); // and the hit path converges from here
}

#[test]
fn prime_drives_a_sync_after_the_prime_chain() {
    // §12/§13 gap (A): prime is an orchestrator of syncs — after the prime chain
    // it must drive `sync` so an established checkout is brought current. Core
    // narrates a `begin` per op at `debug` (§4), so the probe opts into that
    // level; a `sync` op record in the op-log proves the driven sync ran (the
    // chain is tracker-free, so the fetch itself no-ops).
    let tmp = TempDir::new().unwrap();
    let e = Edge { log_level: Some("debug".into()), ..edge(&tmp, None) };
    prime(&e, &argv(&["--as", "me"])).unwrap();
    let log = op_log(&e);
    assert!(log.contains("\"op\":\"prime\""), "prime chain ran: {log}");
    assert!(log.contains("\"op\":\"sync\""), "prime drove a sync: {log}");
}

#[test]
fn a_founding_primes_seed_prune_note_lands_in_the_op_log() {
    // bl-b1be: the founding seed's hinted prune note rides the ordinary log
    // path — persisted in the per-clone op-log file (not a bare eprintln the
    // file never sees), at `info` like install's dangling report, once prime
    // has a Log to give it. The hintless prune stays silent.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let seed = e.xdg.default_config();
    std::fs::create_dir_all(&seed).unwrap();
    std::fs::write(
        seed.join("plugins.toml"),
        "[hooks]\n\"close.pre\" = [\"ghost\", \"mute\"]\n[source]\nghost = \"cargo install ghost\"\n",
    )
    .unwrap();
    prime(&e, &argv(&["--as", "me"])).unwrap();
    let log = op_log(&e);
    assert!(
        log.contains("seed: pruned ghost (no binary beside bl) — source: cargo install ghost — re-add with bl conf after acquiring"),
        "{log}"
    );
    assert!(log.contains("\"lvl\":\"info\""), "the note is info-level: {log}");
    assert!(!log.contains("pruned mute"), "hintless prune stays silent: {log}");
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
    let (b, _) = bind(&e, &l, &s, None, Some(crate::LANDING_BRANCH.into())).unwrap();
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
    let (default_b, _) = bind(&e, &l, &s, None, None).unwrap();
    let (named_b, _) = bind(&e, &l, &s, None, Some("federation/shared".into())).unwrap();
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
fn prime_accepts_the_per_op_remote_override() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    // --remote names the store remote for this op; the empty (tracker-free) chain
    // ignores it, so this just proves it parses and resolves into the binding.
    prime(&e, &argv(&["--remote", "git@hub:r"])).unwrap();
}

#[test]
fn prime_center_writes_the_durable_binding_before_adopt() {
    // bl-35e5: `--center URL` ENROLLS — it writes the per-clone `task-remote`
    // binding (what `conf set task-remote URL` does) BEFORE adopting config, so
    // the §12 ladder resolves the center on this op and every later one. With no
    // tracker (exe_dir None) the adopt half then fails for lack of an install.pre
    // fetch plugin — but the durable binding is already written, which is exactly
    // the resume-idempotent story (re-running converges once a tracker exists). The
    // full happy path (real fetch + adopt + prime) is `tests/dispatch.rs`.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let err = prime(&e, &argv(&["--center", "git@hub:c", "--as", "me"])).unwrap_err();
    assert!(err.to_string().contains("install.pre"), "adopt needs a fetch plugin: {err}");
    // The binding landed first, so a plain later bind resolves the center durably.
    let (l, s) = (landing(&e), store(&e));
    let (b, _) = bind(&e, &l, &s, None, None).unwrap();
    assert_eq!(b.remote.as_deref(), Some("git@hub:c"));
}

#[test]
fn prime_rejects_center_with_install() {
    // bl-35e5: --center subsumes --install (both name the center whose config we
    // adopt), so passing both is refused loud rather than guessing a winner.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let err = prime(&e, &argv(&["--center", "git@hub:c", "--install", "git@hub:c"])).unwrap_err();
    assert!(err.to_string().contains("--center already adopts"), "{err}");
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
fn a_stealth_prime_writes_the_landing_sentinel_that_binds_every_later_op() {
    // §12/bl-9df0: `--stealth` is a DURABLE config act — sugar for `conf set
    // task-remote none` — never a per-invocation flag. The sentinel outranks
    // even the per-machine XDG `remote` (the one remote tier the parse cannot
    // forbid) on EVERY later bind, and a later `--stealth` prime on an
    // ESTABLISHED landing is the same §4 "by you" write.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--as", "me"])).unwrap();
    let user_config = e.xdg.user_config();
    std::fs::create_dir_all(user_config.parent().unwrap()).unwrap();
    std::fs::write(&user_config, "remote = \"git@hub:r\"\n").unwrap();
    let (l, s) = (landing(&e), store(&e));
    let (tracked, _) = bind(&e, &l, &s, None, None).unwrap();
    assert_eq!(tracked.remote.as_deref(), Some("git@hub:r")); // XDG tier resolves
    assert!(!tracked.stealth);
    prime(&e, &argv(&["--stealth", "--as", "me"])).unwrap(); // the established-landing write
    let (stealth, _) = bind(&e, &l, &s, None, None).unwrap(); // a PLAIN later bind
    assert_eq!(stealth.remote, None); // the sentinel stops resolution above origin
    assert!(stealth.stealth);
    // Consent given supersedes withheld — for that one op (the per-op tier).
    let (overridden, _) = bind(&e, &l, &s, Some("git@hub:x".into()), None).unwrap();
    assert_eq!(overridden.remote.as_deref(), Some("git@hub:x"));
    assert!(!overridden.stealth);
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
    // The ONE §12 ladder (bl-c2de): sync takes `--remote` exactly as prime does;
    // the plugin-less chain ignores it, so this proves parse+bind. `--center` is
    // prime-only (enrollment, bl-35e5), so sync bounces it as an unknown flag.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    sync(&e, &argv(&["--remote", "git@hub:r"])).unwrap();
    assert!(sync(&e, &argv(&["--remote"])).is_err()); // flag with no value
    let err = sync(&e, &argv(&["--center", "git@hub:c"])).unwrap_err();
    assert!(err.to_string().contains("unexpected flag '--center'"), "{err}");
}
