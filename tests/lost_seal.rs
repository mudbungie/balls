//! bl-a5f3: a LOST STORE SEAL must unwind like any other abort.
//!
//! `Git::seal` (§8) is two acts — `add -A` + `commit` in the change worktree,
//! then `merge --ff-only` on the store checkout. When a concurrent `bl` advances
//! the store branch first, the ff loses: the checkout is restored (bl-07d6) and
//! `seal()` returns `Err`, but the CHANGE WORKTREE is left committed and CLEAN
//! and no seal record exists. So the engine unwinds as a PRE-abort whose
//! rollback wire carries no `bl-id` trailer — the THIRD state (committed, not
//! integrated, no seal record) the bl-430e metadata fix never anticipated.
//!
//! While `bl-delivery` re-derived the ball by scanning the change worktree for
//! the single changed `tasks/<id>.md`, that scan found ZERO here and the healthy
//! unwind reported `expected exactly one changed task file, found 0` followed by
//! `plugin bl-delivery rollback failed … its close.pre side effects may not be
//! unwound` — telling the operator to distrust a state that is fine. The fix is
//! §0 obligation 4: identity is an op INPUT (`command.id`), carried on every
//! wire, so pre / failed-seal / post all name the same ball.
//!
//! The race is made deterministic rather than timed: a conformant `close.pre`
//! plugin wired AFTER `bl-delivery` commits to the store checkout, which is
//! exactly the state a sibling's winning seal leaves behind — core's own seal
//! then cannot fast-forward.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command as Sys, Output};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// `git -C <dir> <args>`, asserting success (fixture setup with plain git).
fn git(dir: &Path, args: &[&str]) {
    let ok = Sys::new("git").arg("-C").arg(dir).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

/// `git -C <dir> <args>` capturing trimmed stdout.
fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Sys::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} in {}", dir.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// An isolated substrate: a project repo plus the pinned HOME/XDG that keep its
/// clone bundle out of the real `$HOME`.
struct Env {
    home: PathBuf,
    state: PathBuf,
    project: PathBuf,
}

impl Env {
    /// A configured (unrun) `bl`. The `BALLS_*` recursion bookkeeping is scrubbed
    /// so a top-level `bl` here starts at depth 0 (this suite itself runs inside
    /// a close gate).
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

    fn clone_dir(&self) -> balls::layout::CloneDir {
        balls::layout::Xdg::with(&self.home, None, Some(&self.state.to_string_lossy())).clone_dir(&self.project)
    }

    /// The materialized local store checkout (its HEAD is the `balls/tasks` tip).
    fn store_dir(&self) -> PathBuf {
        self.clone_dir().store()
    }

    /// The live (open) balls, as bedrock rows.
    fn live(&self) -> Vec<Value> {
        let out = self.ok(&["list", "--json"]);
        serde_json::from_str(if out.is_empty() { "[]" } else { &out }).unwrap()
    }

    /// Drop an executable `sibling` plugin — a conformant §6 binary that, ONCE,
    /// lands a commit on the store branch under the running op — and bind it in
    /// the landing's `plugins/bin`. That commit is what a concurrent `bl`'s
    /// winning seal leaves behind, so core's own seal cannot fast-forward.
    fn wire_sibling(&self, once: &Path) {
        let bin = self.clone_dir().landing().join("config").join("plugins").join("bin");
        fs::create_dir_all(&bin).unwrap();
        let script = format!(
            "#!/bin/sh\n\
             if [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":[\"close\"]}}'; exit 0; fi\n\
             if [ ! -e {once} ]; then\n  : > {once}\n  \
             git -C {store} commit -q --allow-empty -m 'a sibling op sealed first' >&2\n\
             fi\nexit 0\n",
            once = once.display(),
            store = self.store_dir().display(),
        );
        let path = bin.join("sibling.sh");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&path, bin.join("sibling")).unwrap();
    }
}

/// Give `dir` a committer identity — the delivery squash's `commit-tree` reads
/// the project repo's git config.
fn identity(dir: &Path) {
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "user.email", "t@t"]);
}

/// A primed, stealth substrate over a fresh project repo.
fn env(tmp: &Path) -> Env {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    identity(&project);
    git(&project, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let e = Env { home, state, project };
    let out = e.bl(&["prime", "--as", "a", "--stealth"]);
    assert!(out.status.success(), "prime failed: {}", String::from_utf8_lossy(&out.stderr));
    e
}

#[test]
fn a_lost_store_seal_unwinds_with_the_ball_off_the_wire_and_the_retry_converges() {
    let tmp = TempDir::new().unwrap();
    let e = env(tmp.path());
    let project = e.project.clone();

    // A ball with real work in its delivery worktree.
    let id = e.ok(&["create", "Pave me", "--as", "a"]);
    let wt = PathBuf::from(e.ok(&["claim", &id, "--as", "a"]));
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", &format!("add feature [{id}]")]);

    // Wire the sibling AFTER bl-delivery in close.pre, so bl-delivery has
    // already squashed (and is in the unwind trace) when the seal loses.
    e.wire_sibling(&tmp.path().join("once"));
    e.ok(&["conf", "append", "close.pre", "sibling"]);

    let out = e.bl(&["close", &id, "--as", "a"]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!out.status.success(), "a lost seal must abort the close: {err}");

    // The unwind is CLEAN. Identity came off `command.id`, so no hook went
    // looking in the change worktree — the whole "found 0 ⇒ FAILED ROLLBACK"
    // false alarm is gone by construction (bl-a5f3).
    assert!(!err.contains("changed task file"), "identity was re-derived from the worktree: {err}");
    assert!(!err.contains("rollback failed"), "a healthy unwind reported failure: {err}");

    // And the state really is fine: the delivery squash STANDS on main (it is
    // the BINDING commit point, §14), while the ball is still open and claimed —
    // the store never took the seal.
    let subject = git_out(&project, &["log", "-1", "--format=%s", "main"]);
    assert!(subject.contains(&format!("[{id}]")), "the squash stands on main: {subject}");
    let held = e.live();
    let t = held.iter().find(|t| t["id"] == id.as_str()).expect("the ball is still open");
    assert_eq!(t["claimant"], "a", "the ball stays claimed");

    // Converge-on-retry (§14): the sibling fires only once, so the retried close
    // seals — and it detects its own standing delivery by the `[id]` tag instead
    // of minting a duplicate (bl-430e).
    e.ok(&["close", &id, "--as", "a"]);
    assert!(e.live().iter().all(|t| t["id"] != id.as_str()), "the retried close archived the ball");
    let count = git_out(&project, &["rev-list", "--count", "--fixed-strings", &format!("--grep=[{id}]"), "main"]);
    assert_eq!(count, "1", "exactly one delivery for the ball");
}
