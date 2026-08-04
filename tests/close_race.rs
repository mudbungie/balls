//! bl-a3bb: two concurrent closes in ONE shared project checkout must both land
//! on `main`. `bl close` never pushes the code remote, so a documented bot pool
//! sharing a single checkout coordinates only through claim occupancy — the
//! delivery squash's ref move is what must be race-safe on its own.
//!
//! `delivery_repo_acts.rs::deliver` reads the integration tip, squashes onto it,
//! then moves the ref — a check-then-act window. The fix passes the pre-read tip
//! as `update-ref`'s COMPARE-AND-SWAP old-value (`commit_swap`), so the write
//! lands only while `integration` still points there; if a sibling close moved it
//! in between, git rejects the write (exit != 0) and delivery aborts LOUDLY
//! pre-seal — nothing overwritten, the task stays claimed. The retried close then
//! REFUSES as a stale source (bl-a1a4) until the loser incorporates the winner's
//! tip in their own worktree and tests it there; that retry converges (§14).
//!
//! This test makes the interleave DETERMINISTIC, not timing-based: a `git` shim on
//! PATH freezes actor A precisely at its delivery `update-ref refs/heads/main`,
//! AFTER A has computed its stale parent and its squash. While A is frozen, actor
//! B closes to completion and lands on main. Releasing A now runs its CAS with the
//! stale old-value — git REJECTS it, A's close exits non-zero and stays claimed,
//! and B's squash is untouched. A's next close (no shim) REFUSES the stale source;
//! once A merges B's tip into its own worktree it delivers onto it, so BOTH
//! squashes end as ancestors of final main — nothing dropped.

#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use balls::delivery_path::worktree_path;
use balls::layout::Xdg;
use tempfile::TempDir;

/// `git -C cwd <args>` with plain git, asserting success (harness setup).
fn git(cwd: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// `git -C cwd <args>` stdout, trimmed — a state probe that never asserts.
fn out(cwd: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// Exit-code predicate for a git query (`--is-ancestor`, `cat-file -e`).
fn git_ok(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success()
}

/// A throwaway project repo on `main` with a seed commit.
fn project(tmp: &Path) -> PathBuf {
    let root = tmp.join("proj");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "test"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    fs::write(root.join("seed.txt"), "seed\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "seed"]);
    root
}

/// A change worktree seeding then deleting `tasks/<id>.md` — the close.pre cwd
/// shape of a real close (the ball itself rides the wire, bl-a5f3).
fn change_dir(tmp: &Path, name: &str, id: &str) -> PathBuf {
    let change = tmp.join(name);
    fs::create_dir(&change).unwrap();
    git(&change, &["init", "-q", "-b", "balls"]);
    git(&change, &["config", "user.name", "test"]);
    git(&change, &["config", "user.email", "test@example.com"]);
    fs::create_dir(change.join("tasks")).unwrap();
    fs::write(change.join("tasks").join(format!("{id}.md")), "x\n").unwrap();
    git(&change, &["add", "-A"]);
    git(&change, &["commit", "-qm", "seed"]);
    fs::remove_file(change.join("tasks").join(format!("{id}.md"))).unwrap();
    change
}

fn post(inv: &str, id: &str, title: &str) -> String {
    format!(
        r#"{{"binding":{{"invocation_path":"{inv}"}},"current_state":{{"title":"{title}"}},"metadata":{{"bl-id":["{id}"]}}}}"#
    )
}

fn pre(inv: &str, id: &str, title: &str) -> String {
    format!(
        r#"{{"binding":{{"invocation_path":"{inv}"}},"command":{{"op":"close","id":"{id}"}},"current_state":{{"title":"{title}"}}}}"#
    )
}

/// Install a `git` shim in its own dir and return that dir. The shim passes every
/// call through to `$REAL_GIT` EXCEPT a delivery `update-ref refs/heads/main` made
/// with `$BLOCK_DIR` set: it touches `$BLOCK_DIR/reached` and spins until
/// `$BLOCK_DIR/release` appears, THEN runs the real CAS write — freezing that one
/// actor at its ref move with its stale parent already committed, so on release
/// the compare-and-swap rejects. Written once at setup (well before any spawn),
/// sidestepping the write-then-exec ETXTBSY class.
fn install_git_shim(tmp: &Path, real_git: &str) -> PathBuf {
    let dir = tmp.join("shim");
    fs::create_dir(&dir).unwrap();
    let shim = dir.join("git");
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do\n  [ \"$a\" = update-ref ] && ur=1\n  [ \"$a\" = refs/heads/main ] && rm=1\ndone\n\
         if [ -n \"$BLOCK_DIR\" ] && [ -n \"$ur\" ] && [ -n \"$rm\" ]; then\n  : > \"$BLOCK_DIR/reached\"\n  \
         while [ ! -e \"$BLOCK_DIR/release\" ]; do sleep 0.02; done\nfi\nexec \"{real_git}\" \"$@\"\n"
    );
    fs::write(&shim, script).unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

/// Spin until `path` exists or `secs` elapse.
fn wait_for(path: &Path, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Reap `child` within `secs`, killing it on timeout so the suite never hangs.
fn reap(child: &mut Child, secs: u64) -> Option<ExitStatus> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            return Some(s);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Claim `id` (materialize its work worktree) and drop `file` of work into it.
fn claim_and_work(root: &Path, home: &Path, xdg: &Xdg, inv: &str, id: &str, title: &str, file: &str) -> PathBuf {
    let bin = assert_cmd::cargo::cargo_bin("bl-delivery");
    let ok = Command::new(&bin)
        .current_dir(root)
        .env("BALLS_PLUGIN_NAME", "delivery")
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join("state"))
        .args(["claim", "post"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().unwrap().write_all(post(inv, id, title).as_bytes()).unwrap();
            c.wait()
        })
        .unwrap();
    assert!(ok.success(), "claim.post {id} failed");
    let wt = worktree_path(xdg, "delivery", inv, id);
    fs::write(wt.join(file), "shipped\n").unwrap();
    wt
}

/// Run a close.pre foreground to completion (no block), returning
/// `(succeeded, stderr)` — the refusal path reads the voice, not just the code.
fn close_pre(change: &Path, home: &Path, inv: &str, id: &str, title: &str) -> (bool, String) {
    let bin = assert_cmd::cargo::cargo_bin("bl-delivery");
    let mut c = Command::new(&bin)
        .current_dir(change)
        .env("BALLS_PLUGIN_NAME", "delivery")
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join("state"))
        .args(["close", "pre"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(pre(inv, id, title).as_bytes()).unwrap();
    let mut err = String::new();
    let mut pipe = c.stderr.take().unwrap();
    std::io::Read::read_to_string(&mut pipe, &mut err).unwrap();
    (reap(&mut c, 60).expect("close.pre hung").success(), err)
}

/// [`close_pre`] asserting success — the ordinary delivering close.
fn close_now(change: &Path, home: &Path, inv: &str, id: &str, title: &str) {
    let (ok, err) = close_pre(change, home, inv, id, title);
    assert!(ok, "close.pre {id} failed: {err}");
}

#[test]
fn concurrent_closes_in_one_clone_abort_the_loser_and_keep_both_deliveries() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap().to_string();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let main0 = out(&root, &["rev-parse", "main"]);

    // Two tasks, each claimed into its own work worktree with its own file.
    claim_and_work(&root, &home, &xdg, &inv, "bl-a", "Feature A", "feat_a.txt");
    claim_and_work(&root, &home, &xdg, &inv, "bl-b", "Feature B", "feat_b.txt");
    let change_a = change_dir(tmp.path(), "change_a", "bl-a");
    let change_b = change_dir(tmp.path(), "change_b", "bl-b");

    let real_git = String::from_utf8(Command::new("sh").args(["-c", "command -v git"]).output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    let shim_dir = install_git_shim(tmp.path(), &real_git);
    let block_dir = tmp.path().join("block");
    fs::create_dir(&block_dir).unwrap();
    let path_with_shim = format!("{}:{}", shim_dir.display(), std::env::var("PATH").unwrap_or_default());

    // Actor A: close.pre in the background, frozen at its `update-ref refs/heads/main`
    // by the shim — parent (main0) and squash already computed, write not yet done.
    let bin = assert_cmd::cargo::cargo_bin("bl-delivery");
    let mut a = Command::new(&bin)
        .current_dir(&change_a)
        .env("BALLS_PLUGIN_NAME", "delivery")
        .env("HOME", &home)
        .env("XDG_STATE_HOME", home.join("state"))
        .env("PATH", &path_with_shim)
        .env("REAL_GIT", &real_git)
        .env("BLOCK_DIR", &block_dir)
        .args(["close", "pre"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    a.stdin.take().unwrap().write_all(pre(&inv, "bl-a", "Feature A").as_bytes()).unwrap();

    if !wait_for(&block_dir.join("reached"), 60) {
        let _ = a.kill();
        let _ = a.wait();
        panic!("actor A never reached its delivery update-ref");
    }
    // A is frozen at line 133 with parent==main0. main has NOT moved yet.
    assert_eq!(out(&root, &["rev-parse", "main"]), main0, "A must not have written main yet");

    // Actor B closes fully and lands its squash on main (parent still main0).
    close_now(&change_b, &home, &inv, "bl-b", "Feature B");
    let b_tip = out(&root, &["rev-parse", "main"]);
    assert_ne!(b_tip, main0, "B delivered");
    assert!(out(&root, &["log", "-1", "--format=%s", "main"]).contains("[bl-b]"), "B's squash is on main");
    assert_eq!(out(&root, &["show", "main:feat_b.txt"]), "shipped");

    // Release A: its stale-parent CAS update-ref is now REJECTED by git — the
    // delivery aborts pre-seal. Reap before asserting so a failed assertion can
    // never leave A blocked and the suite hung.
    fs::write(block_dir.join("release"), "go\n").unwrap();
    let a_status = reap(&mut a, 60).expect("actor A close.pre hung after release");
    assert!(!a_status.success(), "actor A close.pre must abort loudly on the rejected CAS");
    // Nothing overwritten: B's squash still IS main, and A never landed.
    assert_eq!(out(&root, &["rev-parse", "main"]), b_tip, "the rejected CAS left B's squash on main");
    assert!(!git_ok(&root, &["cat-file", "-e", "main:feat_a.txt"]), "A's stale squash never landed");

    // A's task is still claimed (the abort never sealed). Its retry — no shim
    // now — REFUSES: a lost CAS does not license delivery to reconcile on A's
    // behalf, and B's tip is not in work/bl-a (bl-a1a4).
    let (ok, err) = close_pre(&change_a, &home, &inv, "bl-a", "Feature A");
    assert!(!ok, "the retry must refuse a stale source");
    assert!(err.contains("stale source") && err.contains("work/bl-a"), "{err}");
    assert_eq!(out(&root, &["rev-parse", "main"]), b_tip, "the refusal moved nothing");

    // A incorporates B's tip in its OWN worktree, tests there, and closes.
    git(&worktree_path(&xdg, "delivery", &inv, "bl-a"), &["merge", "-q", "--no-edit", "main"]);
    close_now(&change_a, &home, &inv, "bl-a", "Feature A");

    // BOTH deliveries survive: final main carries [bl-a] on top of B's [bl-b],
    // and every task's file is present — nothing was dropped.
    let final_tip = out(&root, &["rev-parse", "main"]);
    assert!(out(&root, &["log", "-1", "--format=%s", "main"]).contains("[bl-a]"), "A's retry is the tip");
    assert_eq!(out(&root, &["rev-parse", "main^"]), b_tip, "A's squash now parents B's squash");
    assert!(
        git_ok(&root, &["merge-base", "--is-ancestor", &b_tip, &final_tip]),
        "B's squash {b_tip} is an ancestor of final main {final_tip}"
    );
    assert_eq!(out(&root, &["show", "main:feat_a.txt"]), "shipped");
    assert_eq!(out(&root, &["show", "main:feat_b.txt"]), "shipped");
}
