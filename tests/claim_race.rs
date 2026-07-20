//! End-to-end DIRECTION lock for cross-clone claim contention (bl-0d8c).
//!
//! §13 says "a non-ff IS the contention signal": one fetch+ff-only is the atomic
//! detect-and-act, and the store push is where a stale writer collides. That is
//! proven for `close` (tests/half_close.rs) but never for `claim` — yet claim is
//! where two agents ACTUALLY race: both see the ball unclaimed at their own head,
//! both pass the LOCAL occupancy guard, and only the shared remote can arbitrate.
//!
//! Two clones share a store remote. A claims X and publishes; a STALE B (has X,
//! lacks A's claim) claims X — its local occupancy check passes, so `claim.post =
//! [bl-delivery, bl-tracker]` mints B's worktree, then the tracker's push is
//! rejected non-ff. Because the seal never published, core UN-SEALS and rolls the
//! worktree back (§14 converge-on-retry: the claim is a BINDING effect that never
//! bound, the worktree a NON-BINDING one that recomputes). B is left CLEAN — no
//! local claim, no worktree, no `work/<id>` branch — never a both-think-they-own
//! state and never a silent overwrite of A. The documented recovery (`bl sync`
//! then retry) fast-forwards B onto A's claim and the retry surfaces the ONE
//! claimant-keyed refusal — OCCUPANCY, naming A. A `claim.post` reorder (push
//! before the worktree) or a union/force sync would flip this; driving the real
//! `bl` + both shipped plugins against a shared remote catches it.

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir (its clone bundle + worktrees never touch the real `$HOME`); the
/// shipped plugins resolve beside the built `bl`. Inherited plugin-chain env is
/// scrubbed so a run from inside the close-hook chain can't leak depth/name.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// `git -C <cwd> <args>`, asserting success (harness setup with plain git).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// `git -C <cwd> <args>` capturing trimmed stdout (a store tip / branch list).
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed in {}", cwd.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// The stored `claimant` field via the lossless `show --json` mirror (§3):
/// `Value::Null` when unclaimed, a `Value::String` otherwise.
fn claimant(project: &Path, home: &Path, state: &Path, id: &str) -> serde_json::Value {
    let json = stdout(bl(project, home, state).args(["show", id, "--json"]).assert().success());
    serde_json::from_str::<serde_json::Value>(&json).unwrap()["claimant"].clone()
}

/// A clone that shares `origin`'s store remote, its `bl` HOME/XDG isolated.
struct Clone {
    project: PathBuf,
    home: PathBuf,
    state: PathBuf,
}

impl Clone {
    /// Clone `origin`, stamp a git identity, prime (found/adopt the store), and
    /// isolate `bl`'s HOME/XDG under `<tmp>/<name>-{h,s}`.
    fn new(tmp: &Path, origin: &Path, name: &str) -> Self {
        let project = tmp.join(name);
        git(tmp, &["clone", "-q", &origin.to_string_lossy(), &project.to_string_lossy()]);
        git(&project, &["config", "user.name", name]);
        git(&project, &["config", "user.email", &format!("{name}@e")]);
        let c = Clone { project, home: tmp.join(format!("{name}-h")), state: tmp.join(format!("{name}-s")) };
        c.bl(&["prime"]).assert().success();
        c
    }
    fn bl(&self, args: &[&str]) -> Command {
        let mut cmd = bl(&self.project, &self.home, &self.state);
        cmd.args(args);
        cmd
    }
    fn claimant(&self, id: &str) -> serde_json::Value {
        claimant(&self.project, &self.home, &self.state, id)
    }
}

/// A bare origin hosting `main` + the store, plus Alice's primed clone with a
/// published task `X`. Returns `(tmp, origin, alice, xid)`; Bob clones later so
/// he is STALE w.r.t. whatever Alice does after his prime.
fn published_task(name: &str) -> (TempDir, PathBuf, Clone, String) {
    let tmp = TempDir::new().unwrap();
    let origin = tmp.path().join("origin.git");
    git(tmp.path(), &["init", "--bare", "-q", "-b", "main", &origin.to_string_lossy()]);
    let seed = tmp.path().join("seed");
    git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "s"]);
    git(&seed, &["config", "user.email", "s@e"]);
    std::fs::write(seed.join("seed.txt"), "seed\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "seed"]);
    git(&seed, &["push", "-q", "origin", "main"]);

    let alice = Clone::new(tmp.path(), &origin, "alice");
    let xid = stdout(alice.bl(&["create", name, "--as", "alice"]).assert().success());
    (tmp, origin, alice, xid)
}

#[test]
fn a_stale_claim_push_is_rejected_non_ff_and_leaves_b_clean() {
    // Both clones hold X unclaimed at their own head. Alice claims + publishes;
    // Bob (primed BEFORE the claim, so he has X but not the claim) then claims —
    // local occupancy passes, delivery mints his worktree, and the tracker push
    // hits the non-ff wall. The sharpened message names the `bl sync` + retry
    // recovery, and the abort un-seals Bob back to CLEAN.
    let (tmp, origin, alice, xid) = published_task("Contended X");
    let bob = Clone::new(tmp.path(), &origin, "bob"); // has X, not yet Alice's claim
    assert_eq!(bob.claimant(&xid), serde_json::Value::Null, "Bob sees X unclaimed");

    alice.bl(&["claim", &xid, "--as", "alice"]).assert().success();
    let published = git_out(&origin, &["rev-parse", "balls/tasks"]); // the store tip Bob must not clobber

    bob.bl(&["claim", &xid, "--as", "bob"])
        .assert()
        .failure()
        .stderr(contains("push rejected: the remote store moved ahead").and(contains("run `bl sync`, then re-run the command")));

    // FINDING (behaves-as-designed): the rejected `claim.post` push UN-SEALS Bob.
    // He is left with NO local claim, NO worktree, NO `work/<id>` branch — the
    // converge-on-retry clean slate, not a half-claimed leftover.
    assert_eq!(bob.claimant(&xid), serde_json::Value::Null, "un-sealed: Bob holds no local claim");
    let work = format!("work/{xid}");
    assert!(!git_out(&bob.project, &["branch", "--list", &work]).contains(&work), "no orphan work branch");
    let worktrees = git_out(&bob.project, &["worktree", "list"]);
    assert!(!worktrees.contains(&xid), "no orphan worktree:\n{worktrees}");

    // NO SILENT OVERWRITE: Bob's push never landed — the shared store still sits
    // exactly at Alice's claim, so there is no both-think-they-own-it fork.
    assert_eq!(git_out(&origin, &["rev-parse", "balls/tasks"]), published, "store tip unmoved by the rejected push");
}

#[test]
fn b_sync_then_retry_surfaces_the_occupancy_refusal_naming_a() {
    // The documented recovery. After the non-ff reject Bob is un-diverged (the
    // seal never published), so a plain ff-only `bl sync` converges his view onto
    // Alice's claim — and the retry then trips the ONE claimant-keyed guard,
    // OCCUPANCY, naming Alice. Never a silent takeover, never a second owner.
    let (tmp, origin, alice, xid) = published_task("Raced X");
    let bob = Clone::new(tmp.path(), &origin, "bob");
    alice.bl(&["claim", &xid, "--as", "alice"]).assert().success();
    bob.bl(&["claim", &xid, "--as", "bob"]).assert().failure(); // the rejected optimistic claim

    // `bl sync` fast-forwards Bob's store onto Alice's claim (ff-only succeeds
    // precisely because the abort left Bob non-diverged — no union/merge needed).
    bob.bl(&["sync"]).assert().success();
    assert_eq!(bob.claimant(&xid), "alice", "sync corrects Bob's view: Alice owns X");

    // The retry now SEES the incumbent and refuses at author time, naming Alice —
    // the same occupancy wording tests/guards.rs pins, here reached across clones.
    bob.bl(&["claim", &xid, "--as", "bob"])
        .assert()
        .failure()
        .stderr(contains(format!("{xid} is already claimed by alice")));

    // The store is Alice's and only Alice's — Bob authored nothing onto it.
    assert_eq!(claimant(&alice.project, &alice.home, &alice.state, &xid), "alice");
    let subjects = git_out(&origin, &["log", "--format=%s", "balls/tasks"]);
    assert!(subjects.contains("Raced X"), "Alice's task stands on the shared store:\n{subjects}");
}
