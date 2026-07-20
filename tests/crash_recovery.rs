//! §14 converge-on-retry under a REAL process death — SIGKILL of a live `bl`
//! (and its whole plugin chain) suspended mid-close, not a scripted plugin
//! non-zero exit or a hand-made empty debris dir. §14 exists precisely for "the
//! bl process itself dies: nothing runs, not even rollback"; these tests prove
//! the two properties that must survive it — the task lands in exactly ONE of
//! {still-claimed, delivered-and-retired} (never both-gone limbo), a retried
//! close converges on a single `[bl-id]` squash, and `bl prime` REPORTS the real
//! crash debris (the orphan change worktree) rather than choking.
//!
//! Two kill points, mirroring the half-close direction-lock (tests/half_close.rs):
//! - during the `close.pre` delivery GATE, before the squash lands — the task
//!   stays claimed and main never moves;
//! - during `close.post`, AFTER the squash landed and core sealed — the task is
//!   delivered + retired, and a retry mints no duplicate.
//!
//! Everything runs the freshly-built `bl` + its shipped `bl-delivery`/`bl-tracker`
//! siblings against a throwaway stealth repo (no remote). Every wait is bounded so
//! the suite can never hang, and the killed child is always reaped.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use balls::layout::Xdg;
use tempfile::TempDir;

/// A shell body that records it was reached (`touch marker`) then blocks until
/// the process is killed — the known suspension point the SIGKILL lands on.
fn block_body(marker: &Path) -> String {
    format!("touch {}\nsleep 100000\n", marker.display())
}

/// A configured (not-yet-run) `bl` in `project`, HOME/state pinned under the
/// tempdir and the inherited plugin-chain env scrubbed (this suite itself runs
/// inside a close-hook chain — the bl-spawning-test idiom).
fn bl(project: &Path, home: &Path, state: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(assert_cmd::cargo::cargo_bin("bl"));
    c.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME")
        .args(args);
    c
}

/// Run `bl`, return `(success, trimmed stdout, stderr)`.
fn run(mut c: Command) -> (bool, String, String) {
    let o = c.output().unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).trim().to_string(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

/// `git -C cwd <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// Every `main` commit subject in `project`, newest first, one per line.
fn subjects(project: &Path) -> String {
    let o = Command::new("git").arg("-C").arg(project).args(["log", "--format=%s"]).output().unwrap();
    String::from_utf8(o.stdout).unwrap()
}

/// How many delivery squashes carry `[tid]` on main (0, 1, or a duplicate bug).
fn deliveries(project: &Path, tid: &str) -> usize {
    let tag = format!("[{tid}]");
    subjects(project).lines().filter(|l| l.contains(&tag)).count()
}

/// A primed stealth substrate: fresh `main` repo + a founded landing (no remote,
/// so prime runs the shipped tracker/delivery chain end to end).
fn setup(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    for d in [&home, &state, &project] {
        fs::create_dir_all(d).unwrap();
    }
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "t"]);
    git(&project, &["config", "user.email", "t@e"]);
    fs::write(project.join("seed.txt"), "seed\n").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    let (ok, _, e) = run(bl(&project, &home, &state, &["prime"]));
    assert!(ok, "prime: {e}");
    (project, home, state)
}

/// Create + claim a task and commit a feature on its work worktree; returns
/// `(tid, worktree)` primed for a close that would deliver.
fn claim_with_work(project: &Path, home: &Path, state: &Path) -> (String, PathBuf) {
    let (ok, tid, e) = run(bl(project, home, state, &["create", "Ship it", "--as", "me"]));
    assert!(ok, "create: {e}");
    let (ok, wt, e) = run(bl(project, home, state, &["claim", &tid, "--as", "me"]));
    assert!(ok, "claim: {e}");
    let wt = PathBuf::from(wt.lines().last().unwrap());
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", &format!("feat [{tid}]")]);
    (tid, wt)
}

/// Install `project`'s shared `pre-commit` hook (delivery's close.pre gate runs
/// it on the reintegrated tree).
fn install_hook(project: &Path, body: &str) {
    let hook = project.join(".git/hooks/pre-commit");
    fs::write(&hook, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
}

/// The landing's local `config/plugins/bin/` binding store (where a plugin name
/// resolves to this box's binary).
fn landing_bin(home: &Path, state: &Path, project: &Path) -> PathBuf {
    Xdg::with(home, None, Some(&state.to_string_lossy()))
        .clone_dir(project)
        .landing()
        .join("config")
        .join("plugins")
        .join("bin")
}

/// Wait (bounded) until the killed child is reaped; returns its exit status.
fn reap(mut child: Child) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            return s;
        }
        assert!(Instant::now() < deadline, "killed bl never exited");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Spawn `bl` in its OWN process group, wait (bounded) until it suspends at
/// `marker`, then SIGKILL the whole group (pgid == the child's pid) — the real
/// process death §14 is written for. Reaps the child, asserting it did not exit
/// clean.
fn kill_mid_flight(mut c: Command, marker: &Path) {
    c.process_group(0).stdout(Stdio::null()).stderr(Stdio::null());
    let child = c.spawn().unwrap();
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "close never reached the suspension point");
        std::thread::sleep(Duration::from_millis(50));
    }
    let killed = Command::new("sh").arg("-c").arg(format!("kill -KILL -{pid}")).status().unwrap();
    assert!(killed.success(), "SIGKILL of the process group failed");
    assert!(!reap(child).success(), "a SIGKILLed bl must not exit clean");
}

#[test]
fn kill_during_close_pre_gate_stays_claimed_and_retry_converges() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = setup(tmp.path());
    let (tid, worktree) = claim_with_work(&project, &home, &state);

    // The pre-commit gate blocks forever the moment delivery runs it — SIGKILL
    // suspends the close BEFORE the squash reaches main.
    let marker = tmp.path().join("pre-reached");
    install_hook(&project, &block_body(&marker));
    kill_mid_flight(bl(&project, &home, &state, &["close", &tid, "--as", "me"]), &marker);

    // Exactly ONE of {claimed, delivered}: STILL CLAIMED — no squash, task live,
    // worktree up. Never both-gone limbo; list gives a coherent answer.
    assert_eq!(deliveries(&project, &tid), 0, "no squash landed pre-gate:\n{}", subjects(&project));
    let (ok, json, _) = run(bl(&project, &home, &state, &["list", "--json"]));
    assert!(ok && json.contains(&tid) && json.contains("\"claimant\": \"me\""), "task still claimed: {json}");
    assert!(worktree.exists(), "work worktree left up for the fix");

    // Release the blocker; the retried close CONVERGES — one squash, task archived.
    install_hook(&project, "exit 0\n");
    let (ok, _, e) = run(bl(&project, &home, &state, &["close", &tid, "--as", "me"]));
    assert!(ok, "retry close: {e}");
    assert_eq!(deliveries(&project, &tid), 1, "exactly one delivery, no duplicate:\n{}", subjects(&project));
    let (_, json, _) = run(bl(&project, &home, &state, &["list", "--json"]));
    assert!(!json.contains(&tid), "retry archived the task: {json}");

    // prime REPORTS the real crash debris (the killed op's orphan change worktree)
    // and still succeeds — it does not choke on what the kill left.
    let (ok, _, err) = run(bl(&project, &home, &state, &["prime"]));
    assert!(ok, "prime after kill must not choke: {err}");
    assert!(err.contains("orphan change worktree") && err.contains("crash debris"), "prime names the debris: {err}");
}

#[test]
fn kill_during_close_post_is_delivered_and_retirable_never_limbo() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = setup(tmp.path());

    // A blocker wired LAST in close.post: it runs only AFTER close.pre squashed to
    // main, core sealed the task, and close.post teardown+tracker already ran — so
    // the SIGKILL lands with the delivery already binding.
    let marker = tmp.path().join("post-reached");
    let blocker = tmp.path().join("blocker");
    fs::write(
        &blocker,
        format!(
            "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":[\"close\"]}}'; exit 0; fi\n{}",
            block_body(&marker)
        ),
    )
    .unwrap();
    fs::set_permissions(&blocker, fs::Permissions::from_mode(0o755)).unwrap();
    let bindir = landing_bin(&home, &state, &project);
    fs::create_dir_all(&bindir).unwrap();
    symlink(&blocker, bindir.join("blocker")).unwrap();
    let (ok, _, e) = run(bl(&project, &home, &state, &["conf", "append", "close.post", "blocker"]));
    assert!(ok, "conf append: {e}");

    let (tid, _wt) = claim_with_work(&project, &home, &state);
    kill_mid_flight(bl(&project, &home, &state, &["close", &tid, "--as", "me"]), &marker);

    // DELIVERED + RETIRED (coherent, never both-gone limbo): the squash stands on
    // main exactly once, `list` no longer carries the task, and `bl show` calls it
    // closed.
    assert_eq!(deliveries(&project, &tid), 1, "squash landed once pre-kill:\n{}", subjects(&project));
    let (_, json, _) = run(bl(&project, &home, &state, &["list", "--json"]));
    assert!(!json.contains(&tid), "sealed: no longer an open task: {json}");
    let (ok, show, _) = run(bl(&project, &home, &state, &["show", &tid]));
    assert!(ok && show.contains("closed"), "bl show gives a coherent closed answer: {show}");

    // Retry converges idempotently — with the blocker removed, re-closing an
    // already-delivered+sealed task mints NO duplicate squash.
    // FINDING (real-bug-simple, cosmetic): re-closing a closed/unknown id exits 1
    // with a bare `bl: No such file or directory (os error 2)` — IDENTICAL to
    // closing any nonexistent id in a clean repo (verified out-of-band), NOT crash
    // corruption. The convergence invariant (one squash, no double delivery) holds.
    run(bl(&project, &home, &state, &["conf", "remove", "close.post", "blocker"]));
    let (retry_ok, _, _) = run(bl(&project, &home, &state, &["close", &tid, "--as", "me"]));
    assert!(!retry_ok, "re-closing a sealed task is the no-open-task refusal, not a re-delivery");
    assert_eq!(deliveries(&project, &tid), 1, "retry made no duplicate delivery:\n{}", subjects(&project));

    // prime reports the real crash debris and succeeds.
    let (ok, _, err) = run(bl(&project, &home, &state, &["prime"]));
    assert!(ok, "prime after kill must not choke: {err}");
    assert!(err.contains("orphan change worktree") && err.contains("crash debris"), "prime names the debris: {err}");
}
