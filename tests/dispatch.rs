//! End-to-end harness: build the `bl` binary and run it from a throwaway temp
//! directory, never against the dev repo's own task list. The read verbs
//! (`show`/`list`, §9) render the store; the
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
/// and `state` so its clone bundle lands in the tempdir, not the
/// real `$HOME`. The `tracker` sibling is found beside the built `bl` (§12).
fn bl_primed(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// The landing checkout for an invocation at `project` —
/// `$XDG_STATE_HOME/balls/clones/<pct-enc-project>/config`.
fn landing(state: &Path, project: &Path) -> PathBuf {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(project)
        .landing()
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
    let dir = contains("create").and(contains("unclaim")).and(contains("skill")).and(contains("usage: bl"));
    bl(&TempDir::new().unwrap()).arg("help").assert().success().stdout(dir);
}

#[test]
fn per_command_help_lists_the_commands_own_flags() {
    // bl-7990: `bl <cmd> --help` and `bl help <cmd>` surface a command's own
    // flags + examples — the affordance whose absence left `create --body`
    // undiscoverable. Works with no landing (intercepted before the parser).
    let body = contains("--body").and(contains("usage: bl create")).and(contains("Examples:"));
    bl(&TempDir::new().unwrap()).args(["create", "--help"]).assert().success().stdout(body);
    bl(&TempDir::new().unwrap()).args(["help", "create"]).assert().success().stdout(contains("--subtask-of"));
}

#[test]
fn a_usage_error_appends_the_commands_help() {
    // An arity/parse error now prints the command's help after the terse error
    // (routed by the InvalidInput tag), so a mis-invocation surfaces the flags.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state)
        .arg("create")
        .assert()
        .failure()
        .stderr(contains("expects exactly one positional argument").and(contains("--body")));
}

#[test]
fn prime_founds_a_stealth_landing_and_runs_the_tracker_chain() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();

    // Fresh box, no remote → prime founds the landing AND runs the prime chain,
    // whose tracker handler emits the stealth W1 on stderr (§12, bl-9df0 — the
    // self-lock is gone; stealth persists nothing). The warning is proof the
    // full engine→subprocess→tracker path ran, not just bootstrap.
    bl_primed(&project, &home, &state)
        .arg("prime")
        .assert()
        .success()
        .stderr(contains("store is stealth (local)"));

    // Idempotent: a second prime converges to a no-op, and `sync` runs the
    // tracker's sync/pre against the store.
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    bl_primed(&project, &home, &state).arg("sync").assert().success();
}

#[test]
fn prime_stealth_opts_out_of_founding_on_a_pushable_origin() {
    // §12 `bl prime --stealth` end to end, through the real bl + tracker: in a
    // clone with a pushable `origin`, a plain prime FOUNDS balls/tasks on the
    // remote; `--stealth` writes the landing `task_remote` sentinel instead
    // (bl-9df0) — and the origin is never touched, by prime OR by any later op.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let origin = tmp.path().join("origin.git");
    git(tmp.path(), &["init", "--bare", "-q", "-b", "main", &origin.to_string_lossy()]);
    let project = tmp.path().join("p");
    git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &project.to_string_lossy()]);

    bl_primed(&project, &home, &state).args(["prime", "--stealth"]).assert().success();
    let cfg = std::fs::read_to_string(landing(&state, &project).join("config/balls.toml")).unwrap();
    assert!(cfg.contains("task_remote = \"none\""), "the durable sentinel: {cfg}");
    let founded = || {
        std::process::Command::new("git")
            .args(["-C", &origin.to_string_lossy(), "rev-parse", "--verify", "refs/heads/balls/tasks"])
            .status().unwrap().success()
    };
    assert!(!founded(), "--stealth must not found balls/tasks on origin");

    // The bl-9df0 regression: consent withheld binds EVERY op, not just the
    // prime that declared it — a later mutate must not rediscover origin and
    // implicitly found the store (the write-only-lock bug).
    bl_primed(&project, &home, &state).args(["create", "T", "--as", "me"]).assert().success();
    assert!(!founded(), "a post-stealth mutate must not found balls/tasks on origin");

    // The contradiction is refused loud (§12: stealth means no store remote).
    bl_primed(&project, &home, &state)
        .args(["prime", "--stealth", "--remote", "git@hub:r"])
        .assert().failure().stderr(contains("--stealth contradicts"));
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

