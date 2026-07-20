//! PROBE (bl-860c): two concurrent closes in ONE shared project checkout must
//! both land on `main`. `bl close` never pushes the code remote, so a documented
//! bot pool sharing a single checkout coordinates only through claim occupancy —
//! nothing warns that closing two DIFFERENT tasks at the same instant is unsafe.
//!
//! FINDING — CONFIRMED, ARCHITECTURAL LOST-DELIVERY RACE.
//! `delivery_repo_acts.rs::deliver` is a non-atomic check-then-act on `main`:
//!   line 125  parent = rev-parse(integration)      // read main
//!   line 126  commit = commit-tree(tree, -p parent) // squash onto that parent
//!   line 133  update-ref refs/heads/<integration> commit   // NO old-value arg
//! `git update-ref <ref> <new>` (without the optional `<oldvalue>` compare-and-swap
//! third argument) writes UNCONDITIONALLY. If actor A reads parent=main0, then
//! actor B fully delivers (main0 -> B), then A's update-ref fires, A overwrites
//! main with a commit whose parent is main0 — B's squash is silently dropped off
//! main's history (recoverable only via reflog, and nothing reports it).
//!
//! This test makes the interleave DETERMINISTIC, not timing-based: a `git` shim on
//! PATH blocks actor A precisely at its delivery `update-ref refs/heads/main`
//! (the write in line 133), AFTER A has already computed its parent (line 125) and
//! its squash (line 126). While A is frozen there, actor B closes to completion
//! and lands on main; then A is released and its stale-parent update-ref overwrites
//! B. The assertions PIN the drop.
//!
//! THE FIX SHAPE (not applied here — this probe pins reality for the maintainer):
//! pass the pre-read parent as `update-ref`'s old-value: `update-ref -m <subj>
//! refs/heads/<int> <new> <parent>`. git then rejects the write when main moved
//! under the delivery (exit != 0), turning the silent drop into a loud abort the
//! close can retry (re-fold + re-squash on the new tip), exactly as the store
//! push already does (`half_close.rs`). NOTE the suspected-race brief proposed
//! blocking at the pre-commit gate; that CANNOT reproduce it — the gate runs at
//! line 119, BEFORE the parent read, so a gate-blocked actor re-reads main after
//! the other lands and both survive. The window is strictly 125->133.

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

/// A change worktree seeding then deleting `tasks/<id>.md` — the close.pre cwd,
/// the staged deletion being how the pre hook recovers the id.
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

fn pre(inv: &str, title: &str) -> String {
    format!(r#"{{"binding":{{"invocation_path":"{inv}"}},"current_state":{{"title":"{title}"}}}}"#)
}

/// Install a `git` shim in its own dir and return that dir. The shim passes every
/// call through to `$REAL_GIT` EXCEPT a delivery `update-ref refs/heads/main` made
/// with `$BLOCK_DIR` set: it touches `$BLOCK_DIR/reached` and spins until
/// `$BLOCK_DIR/release` appears, THEN runs the real write — freezing that one
/// actor at line 133 with its stale parent already committed. Written once at
/// setup (well before any spawn), sidestepping the write-then-exec ETXTBSY class.
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

/// Run a close.pre foreground to completion (actor B — no block), asserting success.
fn close_now(change: &Path, home: &Path, inv: &str, title: &str) {
    let bin = assert_cmd::cargo::cargo_bin("bl-delivery");
    let mut c = Command::new(&bin)
        .current_dir(change)
        .env("BALLS_PLUGIN_NAME", "delivery")
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join("state"))
        .args(["close", "pre"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(pre(inv, title).as_bytes()).unwrap();
    assert!(reap(&mut c, 60).expect("actor B close.pre hung").success(), "actor B close.pre failed");
}

#[test]
fn concurrent_closes_in_one_clone_drop_the_first_delivery_off_main() {
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
    a.stdin.take().unwrap().write_all(pre(&inv, "Feature A").as_bytes()).unwrap();

    if !wait_for(&block_dir.join("reached"), 60) {
        let _ = a.kill();
        let _ = a.wait();
        panic!("actor A never reached its delivery update-ref");
    }
    // A is frozen at line 133 with parent==main0. main has NOT moved yet.
    assert_eq!(out(&root, &["rev-parse", "main"]), main0, "A must not have written main yet");

    // Actor B closes fully and lands its squash on main (parent still main0).
    close_now(&change_b, &home, &inv, "Feature B");
    let b_tip = out(&root, &["rev-parse", "main"]);
    assert_ne!(b_tip, main0, "B delivered");
    assert!(out(&root, &["log", "-1", "--format=%s", "main"]).contains("[bl-b]"), "B's squash is on main");
    assert_eq!(out(&root, &["show", "main:feat_b.txt"]), "shipped");

    // Release A: its stale-parent update-ref overwrites main. Reap before asserting
    // so a failed assertion can never leave A blocked and the suite hung.
    fs::write(block_dir.join("release"), "go\n").unwrap();
    let a_status = reap(&mut a, 60).expect("actor A close.pre hung after release");
    assert!(a_status.success(), "actor A close.pre failed");

    // FINDING — the drop. A overwrote main with a commit parented on main0; B's
    // squash (b_tip) is no longer reachable from main, though its object survives
    // (reflog-only recovery, unreported).
    let final_tip = out(&root, &["rev-parse", "main"]);
    assert!(out(&root, &["log", "-1", "--format=%s", "main"]).contains("[bl-a]"), "A is the surviving tip");
    assert_eq!(out(&root, &["show", "main:feat_a.txt"]), "shipped");
    assert_eq!(out(&root, &["rev-parse", "main^"]), main0, "A's squash parents main0, bypassing B");
    assert!(
        !git_ok(&root, &["merge-base", "--is-ancestor", &b_tip, &final_tip]),
        "LOST DELIVERY: B's squash {b_tip} must NOT be an ancestor of main {final_tip}"
    );
    assert!(git_ok(&root, &["cat-file", "-e", &b_tip]), "B's squash survives as a dangling object (reflog only)");
    assert!(!git_ok(&root, &["cat-file", "-e", "main:feat_b.txt"]), "feat_b.txt dropped off main");
}
