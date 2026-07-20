//! `bl install` DIRECTIONS + refusals (bl-0df2), split from the copy-shape tests
//! for the 300-line cap. `--to` may name only the two LOCAL branches (the landing
//! or the configured store); a bare `bl install` adopts the CONFIGURED UPSTREAM,
//! refusing when none is configured; and an install before `prime` refuses BEFORE
//! any write. The upstream-present leg drives the real `bl-tracker` fetch against
//! a local bare center (a filesystem path is a legitimate center), mirroring
//! `tests/enrollment.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

use crate::{bl, git, primed};

/// A BARE center on `balls/config` whose `config/` names `tasks_branch` and wires
/// `bl-tracker` (the only remote-talker) into `install.pre`/`prime.*`/`sync.pre`,
/// so an enrolling satellite can fetch and adopt it — the `tests/enrollment.rs`
/// center. Returns the bare repo path (a legitimate center).
fn center(dir: &Path) -> PathBuf {
    let bare = dir.join("center.git");
    git(dir, &["init", "--bare", "-q", "-b", "balls/config", &bare.to_string_lossy()]);
    let seed = dir.join("center-seed");
    git(dir, &["clone", "-q", &bare.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "c"]);
    git(&seed, &["config", "user.email", "c@c"]);
    fs::create_dir_all(seed.join("config")).unwrap();
    fs::write(seed.join("config/balls.toml"), "tasks_branch = \"balls/tasks\"\n# CENTER-MARKER\n").unwrap();
    fs::write(
        seed.join("config/plugins.toml"),
        "[hooks]\n\"sync.pre\" = [\"bl-tracker\"]\n\"prime.pre\" = [\"bl-tracker\"]\n\"prime.post\" = [\"bl-tracker\"]\n\"install.pre\" = [\"bl-tracker\"]\n",
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "center config"]);
    git(&seed, &["push", "-q", "origin", "balls/config"]);
    bare
}

#[test]
fn a_to_that_names_neither_local_branch_is_refused_naming_both_targets() {
    // §6: install is purely local in core, so `--to` resolves to ONE of the two
    // local checkouts. A third ref is refused, and the message names BOTH valid
    // targets — the landing and the configured store branch.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());

    bl(&project, &home, &state)
        .args(["install", "config", "--from", "balls/config", "--to", "some/other"])
        .assert()
        .failure()
        .stderr(contains("balls/config").and(contains("balls/tasks")));
}

#[test]
fn a_bare_install_without_a_configured_upstream_refuses_naming_pass_from() {
    // §6 bare `bl install` adopts the configured upstream; a stealth box has none,
    // so it refuses — pointing at the remedy, `pass --from <ref>`, never a raw git
    // fatal at materialize.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());

    bl(&project, &home, &state)
        .arg("install")
        .assert()
        .failure()
        .stderr(contains("pass --from <ref>"));
}

#[test]
fn an_install_before_prime_refuses_before_any_write() {
    // There is no landing yet, so install refuses at the door — and the refusal
    // precedes ANY write: no clone bundle is materialized by the failed op.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    fs::create_dir_all(&project).unwrap();

    bl(&project, &home, &state)
        .args(["install", "config", "--from", "balls/config", "--to", "balls/tasks"])
        .assert()
        .failure()
        .stderr(contains("run `bl prime` first"));
    assert!(!crate::clone_at(&state, &project).landing().exists(), "a refused pre-prime install writes nothing");
}

#[test]
fn a_bare_install_with_a_configured_upstream_adopts_it() {
    // The positive of the refusal above: once a center is bound (`prime --center`
    // writes the durable binding + wires `bl-tracker` in), a bare `bl install`
    // resolves the upstream through the tracker fetch and re-adopts its config —
    // the same code path, driven end to end (core itself never reaches a remote).
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    fs::create_dir_all(&project).unwrap();
    let bare = center(tmp.path());

    bl(&project, &home, &state).args(["prime", "--center", &bare.to_string_lossy()]).assert().success();

    // Bare install now succeeds where the stealth box refused: it adopts the
    // upstream's config idempotently (the mirror re-copies, nothing deleted).
    bl(&project, &home, &state).arg("install").assert().success().stdout(contains("install:").and(contains("0 deleted")));
    let cfg = fs::read_to_string(crate::clone_at(&state, &project).landing().join("config/balls.toml")).unwrap();
    assert!(cfg.contains("CENTER-MARKER"), "bare install adopted the center's config: {cfg}");
}
