//! End-to-end coverage of the §12 remote-ladder EDGE diagnostics and the `prime`
//! flag matrix, driving the freshly-built `bl` + `bl-tracker` against throwaway
//! repos in a `TempDir` (never the dev repo's own task list). Its own ~50-line
//! harness (`bl_primed`, `git`, `clone_dir`), per the no-shared-harness rule —
//! each `tests/*.rs` is its own crate.
//!
//! What it pins that the sibling e2e files don't:
//! - a per-op `--remote` on a MUTATING verb pushes ONCE, then a plain follow-up
//!   op reverts to the (stealth) ladder and pushes nothing — with NO durable
//!   `binding.toml` ever written (the override shapes one op, bl-c2de);
//! - `prime`'s two EDGE warnings appear verbatim on stderr: W2 (an ephemeral
//!   remote the durable ladder won't reproduce) and the store-elsewhere mismatch
//!   (a center whose config names a different `tasks_branch`);
//! - every `prime` flag CONTRADICTION is refused at parse (`--center`+`--install`,
//!   and `--stealth` against each remote-naming flag);
//! - `prime --as alice` records `alice` in the founding commit's `bl-actor`;
//! - `bl sync <branch>` PULLS the positional branch into a local ref;
//! - `bl doctor` is an unknown verb (usage exit 2 — no doctor verb, ever);
//! - `prime --install CENTER` leaves NO `binding.toml` (adopt-only, not enroll).

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under `home`
/// and `state` so its clone bundle lands in the tempdir, not the real `$HOME`.
/// The shipped `bl-tracker`/`bl-delivery` siblings are found beside the built `bl`.
fn bl_primed(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        // bl-1266: a leaked depth makes the tracker read this shelled `bl` as NESTED
        // and skip its push — the suite runs inside the close hook's plugin chain.
        .env_remove("BALLS_PLUGIN_DEPTH");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// `git -C <cwd> <args>` capturing trimmed stdout (asserting success).
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed in {}", cwd.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The clone bundle (landing/store/binding) for an invocation at `project`.
fn clone_dir(state: &Path, project: &Path) -> balls::layout::CloneDir {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy())).clone_dir(project)
}

/// A fresh git project on `main` with a seed commit and NO `origin` — the stealth
/// shape (the ladder's bottom tier finds nothing to discover).
fn stealth_project(dir: &Path) -> PathBuf {
    let project = dir.join("p");
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "t"]);
    git(&project, &["config", "user.email", "t@t"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    project
}

/// An EMPTY bare repo at `dir/<name>.git` — a reachable remote carrying no branch
/// (so the ladder's edge probes read a live remote, not a fetch error).
fn empty_bare(dir: &Path, name: &str) -> PathBuf {
    let bare = dir.join(format!("{name}.git"));
    git(dir, &["init", "--bare", "-q", "-b", "main", &bare.to_string_lossy()]);
    bare
}

/// A bare repo carrying a `balls/config` branch whose config names `tasks_branch`
/// — the precondition for prime's store-elsewhere diagnostic (bl-9df0/§12).
fn bare_naming_branch(dir: &Path, tasks_branch: &str) -> PathBuf {
    let bare = dir.join("center.git");
    git(dir, &["init", "--bare", "-q", "-b", "balls/config", &bare.to_string_lossy()]);
    let seed = dir.join("center-seed");
    git(dir, &["clone", "-q", &bare.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "c"]);
    git(&seed, &["config", "user.email", "c@c"]);
    std::fs::create_dir_all(seed.join("config")).unwrap();
    std::fs::write(seed.join("config/balls.toml"), format!("tasks_branch = \"{tasks_branch}\"\n# CENTER-MARKER\n")).unwrap();
    std::fs::write(
        seed.join("config/plugins.toml"),
        "[hooks]\n\"install.pre\" = [\"bl-tracker\"]\n\"prime.pre\" = [\"bl-tracker\"]\n",
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "center config"]);
    git(&seed, &["push", "-q", "origin", "balls/config"]);
    bare
}

#[test]
fn an_override_pushes_once_then_reverts_to_the_stealth_ladder() {
    // bl-c2de: a per-op `--remote` is the ladder's TOP tier — it shapes ONE op and
    // writes nothing durable. A mutating `create --remote R` publishes the store to
    // R once; the next plain `create` resolves the (stealth) ladder — no origin, no
    // binding, no XDG — so the tracker's push no-ops and R never advances.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());
    let r = empty_bare(tmp.path(), "r");

    bl_primed(&project, &home, &state).arg("prime").assert().success();
    // The override op: publishes balls/tasks onto R (an empty bare → founding push).
    bl_primed(&project, &home, &state)
        .args(["create", "Pushed via override", "--remote", &r.to_string_lossy(), "--as", "me"])
        .assert()
        .success();
    let tip_after_override = git_out(&r, &["rev-parse", "balls/tasks"]);

    // The follow-up plain op: the ladder reverts to stealth, so R is not touched.
    bl_primed(&project, &home, &state).args(["create", "Local only", "--as", "me"]).assert().success();
    let tip_after_plain = git_out(&r, &["rev-parse", "balls/tasks"]);
    assert_eq!(tip_after_override, tip_after_plain, "a plain op must not push to the override's remote");

    // No durable file was written — the override never becomes the clone's binding.
    assert!(!clone_dir(&state, &project).binding().exists(), "a per-op --remote must not write binding.toml");
}

#[test]
fn prime_on_an_ephemeral_remote_warns_that_plain_commands_wont_reproduce_it() {
    // W2 (bl-c2de): prime acts on an explicit `--remote`, but the DURABLE ladder
    // (binding > XDG > origin) resolves to nothing here (a stealth project) — so
    // plain commands won't reproduce the federation. The literal diagnostic fires.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());
    let r = empty_bare(tmp.path(), "eph");

    bl_primed(&project, &home, &state)
        .args(["prime", "--remote", &r.to_string_lossy()])
        .assert()
        .success()
        .stderr(
            contains("via an explicit remote; the durable ladder (binding > XDG > origin) resolves")
                .and(contains("nothing (plain commands run stealth)")),
        );
}

#[test]
fn prime_warns_when_the_remote_stores_its_tasks_on_a_different_branch() {
    // §12 store-elsewhere: a default-named clone whose remote's config names a
    // NON-default `tasks_branch` is silently-empty unless adopted — prime reads the
    // center's `balls/config`, sees the mismatch, and names the real branch.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());
    let center = bare_naming_branch(tmp.path(), "acme/tasks");

    bl_primed(&project, &home, &state)
        .args(["prime", "--remote", &center.to_string_lossy()])
        .assert()
        .success()
        .stderr(contains("this repo's tasks are on `acme/tasks`").and(contains("bl prime --install")));
}

#[test]
fn prime_refuses_every_remote_flag_contradiction_at_parse() {
    // The parser fails LOUD rather than guessing a winner: --center subsumes
    // --install (mutually exclusive), and --stealth (opt out of any store remote)
    // contradicts each remote-naming flag, in any order.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());

    bl_primed(&project, &home, &state)
        .args(["prime", "--center", "git@hub:c", "--install", "git@hub:c"])
        .assert()
        .failure()
        .stderr(contains("--center already adopts"));

    for (args, needle) in [
        (["--stealth", "--remote", "git@hub:r"], "--stealth contradicts"),
        (["--stealth", "--center", "git@hub:c"], "--stealth contradicts"),
        (["--install", "git@hub:c", "--stealth"], "--stealth contradicts"),
    ] {
        let mut cmd = bl_primed(&project, &home, &state);
        cmd.arg("prime").args(args);
        cmd.assert().failure().stderr(contains(needle));
    }
}

#[test]
fn prime_as_alice_records_the_actor_in_the_founding_commit() {
    // §5: authorship rides the trailer block, not the (deterministic) git identity.
    // `--as alice` founding the store stamps `bl-actor=alice` on the found commit.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());

    bl_primed(&project, &home, &state).args(["prime", "--as", "alice"]).assert().success();

    let store = clone_dir(&state, &project).store();
    let body = git_out(&store, &["log", "-1", "--format=%B", "balls/tasks"]);
    assert!(body.contains("bl-actor: alice"), "founding commit records the --as actor: {body}");
}

#[test]
fn sync_pulls_the_positional_branch_into_a_local_ref() {
    // §13 `bl sync <branch>`: the positional substitutes `tasks_branch` in the
    // binding — the one datum the tracker fetches/ff's — so a named branch present
    // on the remote is pulled into a local ref (here paired with the per-op
    // `--remote` tier so the ladder resolves without an origin).
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());
    let r = empty_bare(tmp.path(), "hub");
    // Seed a `feature/x` branch on the remote.
    let seed = tmp.path().join("hub-seed");
    git(tmp.path(), &["clone", "-q", &r.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "h"]);
    git(&seed, &["config", "user.email", "h@h"]);
    std::fs::write(seed.join("f.txt"), "y").unwrap();
    git(&seed, &["checkout", "-q", "-b", "feature/x"]);
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "feat"]);
    git(&seed, &["push", "-q", "origin", "feature/x"]);
    let remote_tip = git_out(&seed, &["rev-parse", "feature/x"]);

    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state)
        .args(["sync", "feature/x", "--remote", &r.to_string_lossy()])
        .assert()
        .success();

    let store = clone_dir(&state, &project).store();
    let local_tip = git_out(&store, &["rev-parse", "feature/x"]);
    assert_eq!(local_tip, remote_tip, "sync <branch> pulls the positional branch into a local ref");
}

#[test]
fn doctor_is_an_unknown_verb() {
    // MEMORY: no `doctor` verb, ever (bl-77a7). It resolves like any typo — a usage
    // error (exit 2) pointing at `bl help`, never a hidden diagnostic surface.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());

    bl_primed(&project, &home, &state)
        .arg("doctor")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("unknown command 'doctor'").and(contains("bl help")));
}

#[test]
fn prime_install_adopts_config_but_writes_no_durable_binding() {
    // §13: `--install CENTER` adopts the center's config WITHOUT the durable
    // per-clone binding `--center` writes — the difference between adopt-once and
    // enroll. Proof: the config is copied in, yet `binding.toml` stays ABSENT.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let project = stealth_project(tmp.path());
    let center = bare_naming_branch(tmp.path(), "balls/tasks");

    bl_primed(&project, &home, &state)
        .args(["prime", "--install", &center.to_string_lossy()])
        .assert()
        .success()
        .stdout(contains("install:"));

    let clone = clone_dir(&state, &project);
    let cfg = std::fs::read_to_string(clone.landing().join("config/balls.toml")).unwrap();
    assert!(cfg.contains("CENTER-MARKER"), "adopted the center's config: {cfg}");
    assert!(!clone.binding().exists(), "--install must not write a durable binding.toml");
}
