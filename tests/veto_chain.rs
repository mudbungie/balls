//! A third-party veto plugin interleaved with the REAL shipped close chain
//! (bl-f256) — the case every protocol test misses (they drive synthetic plugins
//! on `create.pre` in an otherwise-empty schedule). Here the default seed is
//! intact (`close.pre = [bl-delivery]`, `close.post = [bl-delivery, bl-tracker]`)
//! and we splice one fake plugin in.
//!
//! Story 1 (stealth): a plugin PREPENDED to `close.pre` that exits non-zero is
//! the adversary/review gate — the close aborts before the seal, main never
//! moves, the worktree survives, the task stays claimed; lift the gate and the
//! close lands. Story 2 (real bare remote): the `skill/conf.md` footgun — a
//! failing plugin APPENDED to `close.post` lands AFTER `bl-tracker`, so the store
//! push has already published the seal when the plugin aborts; core un-seals the
//! LOCAL store, which now sits BEHIND the remote, and the next mutate's push is
//! rejected non-fast-forward.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command as Sys, Output};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// A conformant fake plugin: answers `<bin> protocol` (so validation is happy)
/// and exits 1 on every real invocation — the veto. It is never asked to roll
/// back (a failing plugin is not recorded in the unwind trace, §14).
const VETO: &str = "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{\"protocol\":[1],\"ops\":[\"close\",\"create\"]}'; exit 0; fi\nexit 1\n";

/// `git -C <dir> <args>`, asserting success (fixture setup with plain git).
fn git(dir: &Path, args: &[&str]) {
    assert!(Sys::new("git").arg("-C").arg(dir).args(args).status().unwrap().success(), "git {args:?} in {}", dir.display());
}

/// `git -C <dir> <args>` capturing trimmed stdout.
fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Sys::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} in {}", dir.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Whether `git -C <dir> <args>` exits 0 (a probe, not an assert).
fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Sys::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success()
}

/// Give `dir` a committer identity — the delivery squash's `commit-tree` reads
/// the project repo's git config, and a bare seed needs one to commit.
fn identity(dir: &Path) {
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "user.email", "t@t"]);
}

/// An isolated substrate: a project repo plus the pinned HOME/XDG that keep its
/// clone bundle out of the real `$HOME`. The shipped `bl-delivery`/`bl-tracker`
/// resolve beside the built `bl` and are auto-bound by `prime`'s seed.
struct Env {
    home: PathBuf,
    state: PathBuf,
    project: PathBuf,
}

impl Env {
    /// A configured (unrun) `bl`. `BALLS_*` recursion bookkeeping is scrubbed so a
    /// top-level `bl` here starts at depth 0 (this test runs inside a close gate).
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::cargo_bin("bl").unwrap();
        c.current_dir(&self.project)
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("BALLS_PLUGIN_DEPTH")
            .env_remove("BALLS_PLUGIN_NAME")
            .args(args);
        c
    }

    fn bl(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    /// Run `bl`, assert success, return trimmed stdout (a verb's one product).
    fn ok(&self, args: &[&str]) -> String {
        let out = self.bl(args);
        assert!(out.status.success(), "bl {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// This clone's bundle (landing + materialized store) as `bl` resolves it.
    fn clone_dir(&self) -> balls::layout::CloneDir {
        balls::layout::Xdg::with(&self.home, None, Some(&self.state.to_string_lossy())).clone_dir(&self.project)
    }

    /// This clone's landing `config/plugins/bin/` — where a local name binds.
    fn bin_dir(&self) -> PathBuf {
        self.clone_dir().landing().join("config").join("plugins").join("bin")
    }

    /// The materialized local store checkout (its HEAD is the `balls/tasks` tip).
    fn store_dir(&self) -> PathBuf {
        self.clone_dir().store()
    }

    /// Drop the executable `myveto` and bind it beside the shipped plugins.
    fn wire_veto(&self) {
        let bin = self.bin_dir();
        fs::create_dir_all(&bin).unwrap();
        let path = bin.join("myveto.sh");
        fs::write(&path, VETO).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&path, bin.join("myveto")).unwrap();
    }

    /// The live (open) balls, as bedrock rows.
    fn live(&self) -> Vec<Value> {
        let out = self.ok(&["list", "--json"]);
        serde_json::from_str(if out.is_empty() { "[]" } else { &out }).unwrap()
    }
}

/// Create + claim `title`, then commit a feature in the delivery worktree; return
/// `(id, worktree_path)`.
fn work(e: &Env, title: &str) -> (String, PathBuf) {
    let id = e.ok(&["create", title, "--as", "a"]);
    let wt = PathBuf::from(e.ok(&["claim", &id, "--as", "a"]));
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", &format!("add feature [{id}]")]);
    (id, wt)
}

#[test]
fn a_close_pre_veto_blocks_the_real_chain_and_lifting_it_lets_close_land() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    identity(&project);
    git(&project, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let e = Env { home, state, project: project.clone() };
    e.cmd(&["prime", "--as", "a", "--stealth"]).assert_success();

    // Splice the veto FIRST in close.pre; the shipped [bl-delivery] stays behind.
    e.wire_veto();
    e.ok(&["conf", "prepend", "close.pre", "myveto"]);

    let (id, wt) = work(&e, "Pave me");
    let main_before = git_out(&project, &["rev-parse", "main"]);

    // The veto aborts the op BEFORE the seal: bl-delivery never squashes.
    let out = e.bl(&["close", &id, "--as", "a"]);
    assert!(!out.status.success(), "veto must abort close");
    assert!(String::from_utf8_lossy(&out.stderr).contains("myveto aborted the op"), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(git_out(&project, &["rev-parse", "main"]), main_before, "main must not move");
    assert!(wt.exists(), "close.post teardown never ran — worktree survives");
    let held = e.live();
    let t = held.iter().find(|t| t["id"] == id.as_str()).expect("task still open");
    assert_eq!(t["claimant"], "a", "task stays claimed");

    // Lift the gate: the identical close now lands through the real chain.
    e.ok(&["conf", "remove", "close.pre", "myveto"]);
    e.ok(&["close", &id, "--as", "a"]);
    assert!(none_with_id(&e.live(), &id), "task archived");
    let subject = git_out(&project, &["log", "-1", "--format=%s", "main"]);
    assert!(subject.contains(&format!("[{id}]")), "squash on main: {subject}");
    assert_ne!(git_out(&project, &["rev-parse", "main"]), main_before, "main advanced");
    assert!(!wt.exists(), "teardown removed the worktree");
}

#[test]
fn append_after_tracker_publishes_then_unseals_and_the_next_push_rejects() {
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));

    // A bare origin seeded with main — the shared project repo + store host.
    let origin = tmp.path().join("origin.git");
    git(tmp.path(), &["init", "--bare", "-q", "-b", "main", &origin.to_string_lossy()]);
    let seed = tmp.path().join("seed");
    git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &seed.to_string_lossy()]);
    identity(&seed);
    fs::write(seed.join("seed.txt"), "seed\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "seed"]);
    git(&seed, &["push", "-q", "origin", "main"]);

    // The working clone: prime founds balls/tasks on origin (real remote ⇒
    // bl-tracker actually pushes).
    let project = tmp.path().join("p");
    git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &project.to_string_lossy()]);
    identity(&project);
    let e = Env { home, state, project: project.clone() };
    e.cmd(&["prime", "--as", "a"]).assert_success();

    // The documented footgun: append AFTER bl-tracker in close.post.
    e.wire_veto();
    e.ok(&["conf", "append", "close.post", "myveto"]);
    let (id, _wt) = work(&e, "Pave me");
    let task_ref = format!("balls/tasks:tasks/{id}.md");

    // close.pre squashes; the seal archives; close.post runs bl-delivery
    // (teardown), bl-tracker (PUSH — remote now sealed), then myveto ABORTS.
    let out = e.bl(&["close", &id, "--as", "a"]);
    assert!(!out.status.success(), "the appended veto aborts the close");
    assert!(String::from_utf8_lossy(&out.stderr).contains("myveto aborted the op"));

    // FINDING (works-as-designed): the footgun manifests EXACTLY as skill/conf.md
    // documents. The push already published, so the SEAL IS ON THE REMOTE — the
    // archived task file is gone from the remote store tip...
    assert!(!git_ok(&origin, &["cat-file", "-e", &task_ref]), "remote store published the archival seal");
    // ...but core's post-abort un-seal rolled the LOCAL store back behind the
    // remote: the task is live + claimed again locally.
    let local = e.live();
    let t = local.iter().find(|t| t["id"] == id.as_str()).expect("local un-seal re-opened the task");
    assert_eq!(t["claimant"], "a");
    // The divergence is real: the local store tip no longer matches the remote's
    // — the un-seal dropped the archival commit the push had already published.
    let local_tip = git_out(&e.store_dir(), &["rev-parse", "HEAD"]);
    let remote_tip = git_out(&origin, &["rev-parse", "balls/tasks"]);
    assert_ne!(local_tip, remote_tip, "local store diverged behind remote");

    // The next mutate seals on the behind tip and its push is rejected non-ff —
    // the same message the half-close recovery names.
    let next = e.bl(&["create", "follow up", "--as", "a"]);
    assert!(!next.status.success(), "the next push is rejected non-ff");
    let err = String::from_utf8_lossy(&next.stderr);
    assert!(err.contains("push rejected: the remote store moved ahead"), "{err}");
    assert!(err.contains("run `bl sync`, then re-run the command"), "{err}");
}

/// No live row carries `id` — the archived predicate (used post-close).
fn none_with_id(rows: &[Value], id: &str) -> bool {
    rows.iter().all(|t| t["id"] != id)
}

/// `Command` → assert-success sugar local to this file (assert_cmd's `Assert`).
trait AssertSuccess {
    fn assert_success(&mut self);
}
impl AssertSuccess for Command {
    fn assert_success(&mut self) {
        let out = self.output().unwrap();
        assert!(out.status.success(), "command failed: {}", String::from_utf8_lossy(&out.stderr));
    }
}
