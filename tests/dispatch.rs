//! End-to-end harness: build the `bl` binary and run it from a throwaway temp
//! directory, never against the dev repo's own task list. The read verbs
//! (`show`/`list`/`ready`/`dep-tree`, §9) render the store; the
//! checkout-lifecycle verbs (`prime`/`sync`/`install`, §6/§12/§13) and the
//! deliverable verbs (`create`/`claim`/`close`, §9) run the real engine + the
//! shipped `tracker` sibling end to end.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The freshly-built `bl`, pinned to run inside an isolated temp dir.
fn bl(workspace: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(workspace.path());
    cmd
}

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under `home`
/// and `state` so its clone bundle + stealth lock land in the tempdir, not the
/// real `$HOME`. The `tracker` sibling is found beside the built `bl` (§12).
fn bl_primed(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// Where the tracker's stealth lock lands for an invocation at `project` —
/// `$XDG_STATE_HOME/balls/clones/<pct-enc-project>/stealth.lock`.
fn stealth_lock(state: &Path, project: &Path) -> PathBuf {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(project)
        .root()
        .join("stealth.lock")
}

#[test]
fn install_seals_a_path_copy_through_the_engine() {
    // §6 standalone install with the real tracker on install.pre (stealth ⇒ no-op
    // fetch): an identical copy converges, still printing the §6 change summary.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state)
        .args(["install", "config", "--from", "balls/config", "--to", "balls/config"])
        .assert().success().stdout(contains("install: ").and(contains("added")));
}

#[test]
fn a_read_verb_on_an_unprimed_checkout_is_an_empty_success() {
    // No store yet ⇒ the silent-empty case (§13): an empty render, not an error.
    let workspace = TempDir::new().unwrap();
    bl(&workspace).arg("list").assert().success().stdout("");
}

#[test]
fn an_unknown_verb_exits_with_a_usage_error() {
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("frobnicate")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("unknown command 'frobnicate'").and(contains("bl help")));
}

#[test]
fn help_prints_the_command_directory_to_stdout() {
    // `bl help` → the terse directory (verb tokens) on stdout, exit 0.
    let dir = contains("create").and(contains("dep-tree")).and(contains("skill")).and(contains("usage: bl"));
    bl(&TempDir::new().unwrap()).arg("help").assert().success().stdout(dir);
}

#[test]
fn prime_founds_a_stealth_landing_and_runs_the_tracker_chain() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();

    // Fresh box, no remote → prime founds the landing AND runs the prime chain,
    // whose tracker handler writes the stealth self-lock (§12). Its presence is
    // proof the full engine→subprocess→tracker path ran, not just bootstrap.
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    assert!(stealth_lock(&state, &project).is_file());

    // Idempotent: a second prime converges to a no-op, and `sync` runs the
    // tracker's sync/pre against the store.
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state).arg("sync").assert().success();
}

#[test]
fn create_seals_a_ball_and_runs_the_mutating_post_chain() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();

    bl_primed(&project, &home, &state).arg("prime").assert().success();
    // A real deliverable op: author → seal → the tracker's mutating post (which
    // no-ops on a stealth binding). Its success proves the whole engine→
    // subprocess→tracker path runs for a mutating verb, not just prime/sync.
    // §9: create prints the minted id ALONE to stdout so `id=$(bl create …)` is
    // a clean capture — nothing else rides the machine channel.
    let out = bl_primed(&project, &home, &state)
        .args(["create", "A real task", "--as", "me"])
        .assert()
        .success();
    let printed = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let printed = printed.trim();

    // The ball file landed on the STORE terminus...
    let tasks = balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(&project)
        .store()
        .join("tasks");
    let balls: Vec<_> = std::fs::read_dir(&tasks).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some()).collect();
    assert_eq!(balls.len(), 1, "create sealed exactly one ball file");
    // ...and stdout is exactly that ball's id (the bl-id trailer of the seal).
    let on_disk = balls[0].file_stem().unwrap().to_string_lossy();
    assert_eq!(printed, on_disk, "create stdout is the minted id, alone");
}

#[test]
fn the_read_verbs_render_a_created_ball_end_to_end() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state)
        .args(["create", "Render me", "-p", "1", "--as", "me"])
        .assert()
        .success();

    // `list` (piped ⇒ non-tty ⇒ plain) shows the ready ball; `--status ready`
    // (the old `bl ready`, §9) agrees.
    bl_primed(&project, &home, &state)
        .arg("list")
        .assert()
        .success()
        .stdout(contains("ready").and(contains("Render me")).and(contains("p1")));
    bl_primed(&project, &home, &state)
        .args(["list", "--status", "ready"])
        .assert()
        .success()
        .stdout(contains("Render me"));

    // `list --json` is a valid one-element array whose timestamp is the literal
    // stored i64 (the lossless export, §3) — never an ISO string.
    let out = bl_primed(&project, &home, &state).args(["list", "--json"]).assert().success();
    let json = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v[0]["title"], "Render me");
    assert!(v[0]["created"].is_i64());
}

/// Run `git -C <cwd> <args>`, asserting success — the integration harness builds
/// the center repo with plain git (no access to the crate-internal runner).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A BARE center carrying a `balls/config` branch whose `config/` names
/// `tasks_branch` and wires the tracker, plus a `# CENTER-MARKER` in `balls.toml`
/// so the adopting side can prove it copied the center's file verbatim.
fn center(dir: &Path) -> PathBuf {
    let bare = dir.join("center.git");
    git(dir, &["init", "--bare", "-q", "-b", "balls/config", &bare.to_string_lossy()]);
    let seed = dir.join("center-seed");
    git(dir, &["clone", "-q", &bare.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "c"]);
    git(&seed, &["config", "user.email", "c@c"]);
    std::fs::create_dir_all(seed.join("config")).unwrap();
    std::fs::write(seed.join("config/balls.toml"), "tasks_branch = \"balls/tasks\"\n# CENTER-MARKER\n").unwrap();
    std::fs::write(
        seed.join("config/plugins.toml"),
        // prime.post wires the tracker's content-settle (founding push on a first
        // prime, then fetch-ff + publish) — without it a fresh clone never founds
        // the remote store (bl-0a23).
        "[hooks]\n\"sync.pre\" = [\"tracker\"]\n\"prime.pre\" = [\"tracker\"]\n\"prime.post\" = [\"tracker\"]\n\"install.pre\" = [\"tracker\"]\n",
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "center config"]);
    git(&seed, &["push", "-q", "origin", "balls/config"]);
    bare
}

#[test]
fn prime_install_adopts_a_centers_config_via_the_tracker_fetch() {
    // §13 end to end: the tracker (install.pre) fetches the center's config, core
    // copies it into the landing, then prime+sync run. Proof the whole
    // engine→subprocess→tracker→core-copy path works — core itself never fetches.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    let bare = center(tmp.path());

    bl_primed(&project, &home, &state)
        .args(["prime", "--install", &bare.to_string_lossy()])
        .assert()
        .success()
        .stdout(contains("install:")); // the change summary prints

    // The landing's config is the center's, copied verbatim (the marker proves it).
    let landing = balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(&project)
        .landing();
    let cfg = std::fs::read_to_string(landing.join("config/balls.toml")).unwrap();
    assert!(cfg.contains("CENTER-MARKER"), "adopted the center's config file: {cfg}");
}

/// The id `bl create` printed alone to stdout (§9).
fn created_id(out: assert_cmd::assert::Assert) -> String {
    String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string()
}

#[test]
fn update_unlinks_a_blocker_so_a_wedged_claim_succeeds() {
    // §10 in-band recovery: a task blocked from claim by an unresolved edge is
    // freed by `bl update --no-needs`, no store-file surgery — the case that
    // keeps the no-cycle-detector deletion (bl-a38e) honest.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    // A real project on `main` so the delivery plugin can fork the `work/<id>` worktree at claim.
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl_primed(&project, &home, &state).arg("prime").assert().success();

    // B can't be claimed until A resolves (default `--needs` op = claim, §10).
    let a = created_id(bl_primed(&project, &home, &state).args(["create", "Blocker A", "--as", "me"]).assert().success());
    let b = created_id(
        bl_primed(&project, &home, &state).args(["create", "Blocked B", "--needs", &a, "--as", "me"]).assert().success(),
    );

    // The wedge: claim refuses, naming the unresolved blocker.
    bl_primed(&project, &home, &state).args(["claim", &b, "--as", "me"]).assert().failure().stderr(contains(a.clone()));

    // Unlink the edge in-band — then the very same claim goes through.
    bl_primed(&project, &home, &state).args(["update", &b, "--no-needs", &a, "--as", "me"]).assert().success();
    bl_primed(&project, &home, &state).args(["claim", &b, "--as", "me"]).assert().success();
}

#[test]
fn update_overwrites_every_field_end_to_end() {
    // The create-only split is gone: title, body, and parent are all editable
    // after the fact through the real CLI → engine → store round-trip (bl-9703).
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl_primed(&project, &home, &state).arg("prime").assert().success();

    // `--body` sets the ball's markdown body at create (not a commit note).
    let p = created_id(bl_primed(&project, &home, &state).args(["create", "Parent", "--as", "me"]).assert().success());
    let id = created_id(
        bl_primed(&project, &home, &state)
            .args(["create", "Old title", "--body", "first draft", "--as", "me"])
            .assert()
            .success(),
    );
    bl_primed(&project, &home, &state).args(["show", &id]).assert().success().stdout(contains("first draft"));

    // Retitle, rewrite the body, and reparent — all in one update.
    bl_primed(&project, &home, &state)
        .args(["update", &id, "--title", "New title", "--body", "rewritten", "--parent", &p, "--as", "me"])
        .assert()
        .success();
    bl_primed(&project, &home, &state)
        .args(["show", &id, "--json"])
        .assert()
        .success()
        .stdout(contains("\"New title\"").and(contains(format!("\"{p}\""))));
    bl_primed(&project, &home, &state).args(["show", &id]).assert().success().stdout(contains("rewritten"));

    // `--no-parent` clears the pointer back to null (bedrock always emits the key).
    bl_primed(&project, &home, &state).args(["update", &id, "--no-parent", "--as", "me"]).assert().success();
    bl_primed(&project, &home, &state)
        .args(["show", &id, "--json"])
        .assert()
        .success()
        .stdout(contains("\"parent\": null"));
}
