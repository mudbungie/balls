//! The textbook two-developers story end to end (bl-8476): two INDEPENDENT
//! bl-primed clones of the SAME project — identical `root_commit`, so one fleet,
//! no `--center` — sharing the task store through their common `origin`. This is
//! the ordinary same-root convergence loop the other two-writer suites never
//! exercise: `half_close` drives one bl clone against a RAW-git second party, and
//! `fleet`/`enrollment` converge DISTINCT-root repos on an explicit center. Here
//! both parties are real `bl` (each on its OWN `HOME`/`$XDG_STATE_HOME`, as two
//! developers on two boxes would be), the store rides `origin` via the tracker's
//! `origin` fallback (`effective_remote`, §12), and every hop goes through the
//! shipped binary. Two stories:
//!
//!   * A creates → claims → closes a ball; B `bl sync`s and SEES each step land
//!     (the create appears, the close removes it). Then `bl prime` on A PRUNES the
//!     settled `work/<id>` branch the close left behind — a `skill/prime.md`
//!     promise ("prune settled work/<id> branches") only ever tested at the plugin
//!     level (`tests/delivery/prime.rs`), now proven through the real CLI.
//!   * Both clones create CONCURRENTLY off one synced base: A publishes first, so
//!     B's optimistic `create.post` push is rejected non-ff (the §13 contention
//!     signal); B `bl sync`s and re-creates, and after a mutual sync BOTH balls
//!     stand in BOTH clones' live lists.
//!
//! Every `tests/*.rs` is its own crate, so the small harness lives here; the
//! shipped `bl-tracker`/`bl-delivery` resolve beside the built `bl` (§12).

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// One developer's checkout: a project working tree plus the PRIVATE
/// `HOME`/`$XDG_STATE_HOME` its clone bundle lands under — two developers never
/// share an XDG root, so A and B each get their own (unlike `fleet`, whose one
/// center enumerates a shared clones dir).
struct Dev {
    project: PathBuf,
    home: PathBuf,
    state: PathBuf,
}

impl Dev {
    /// `bl` rooted in this checkout, XDG pinned under the tempdir and any inherited
    /// plugin-chain env scrubbed (this suite may itself run inside a close-hook
    /// chain — the bl-spawning-test idiom), so a `bl`-under-test reads only its own
    /// isolated context.
    fn bl(&self) -> Command {
        let mut cmd = Command::cargo_bin("bl").unwrap();
        cmd.current_dir(&self.project)
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("BALLS_PLUGIN_DEPTH")
            .env_remove("BALLS_PLUGIN_NAME");
        cmd
    }

    /// The ids in this clone's own live `bl list --json` scope (§13 empty-list "").
    fn live_ids(&self) -> Vec<String> {
        let json = stdout(self.bl().args(["list", "--json"]).assert().success());
        let v: serde_json::Value = serde_json::from_str(if json.trim().is_empty() { "[]" } else { &json }).unwrap();
        v.as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect()
    }

    /// Does the local project repo still carry the `work/<id>` delivery branch?
    fn has_work_branch(&self, id: &str) -> bool {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&self.project)
            .args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/work/{id}")])
            .output()
            .unwrap()
            .status
            .success()
    }
}

/// Run `git -C <cwd> <args>`, asserting success — the harness builds the origin
/// with plain git (no access to the crate-internal runner).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A verb's single stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// Clone `origin` into `<tmp>/<name>` with a distinct git identity (the delivery
/// squash's `commit-tree` reads the project repo's own `user.*`), then hand back a
/// `Dev` whose private XDG roots live under `<tmp>/<name>-{h,s}`.
fn clone_dev(tmp: &Path, origin: &Path, name: &str) -> Dev {
    let project = tmp.join(name);
    git(tmp, &["clone", "-q", &origin.to_string_lossy(), &project.to_string_lossy()]);
    git(&project, &["config", "user.name", name]);
    git(&project, &["config", "user.email", &format!("{name}@example.com")]);
    Dev { project, home: tmp.join(format!("{name}-h")), state: tmp.join(format!("{name}-s")) }
}

/// A bare `origin` seeded with one `main` commit, plus two independent clones A
/// and B of it. A primes FIRST (its `prime.post` founds `balls/tasks` on origin);
/// B primes SECOND (its `prime.pre` adopts the now-established store, cloning it
/// in). Both clones descend from the same `origin/main`, so they stamp the SAME
/// `root_commit` and form ONE fleet with no center. Returns `(A, B)` converged.
fn two_devs(tmp: &Path) -> (Dev, Dev) {
    let origin = tmp.join("origin.git");
    git(tmp, &["init", "--bare", "-q", "-b", "main", &origin.to_string_lossy()]);
    let seed = tmp.join("seed");
    git(tmp, &["clone", "-q", &origin.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "seed"]);
    git(&seed, &["config", "user.email", "seed@example.com"]);
    std::fs::write(seed.join("README.md"), "shared project\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "seed"]);
    git(&seed, &["push", "-q", "origin", "main"]);

    let a = clone_dev(tmp, &origin, "alice");
    a.bl().arg("prime").assert().success(); // founds balls/tasks on origin
    let b = clone_dev(tmp, &origin, "bob");
    b.bl().arg("prime").assert().success(); // adopts the established store
    (a, b)
}

#[test]
fn two_clones_sharing_one_origin_converge_a_full_lifecycle_and_prime_prunes_the_settled_branch() {
    // A drives a whole create→claim→close lifecycle; B, a separate developer on a
    // separate XDG root, `bl sync`s at each step and observes it land through the
    // shared origin store. Finally A's `bl prime` prunes the settled `work/<id>`
    // branch the close deferred (the skill/prime.md promise, via the real CLI).
    let tmp = TempDir::new().unwrap();
    let (a, b) = two_devs(tmp.path());

    // Same root ⇒ one fleet: B's freshly-adopted store starts empty for both.
    assert!(a.live_ids().is_empty() && b.live_ids().is_empty(), "converged empty store");

    // A CREATES → publishes on create.post; B syncs and SEES the new ball.
    let id = stdout(a.bl().args(["create", "Pave the road", "--as", "alice"]).assert().success());
    assert!(id.starts_with("bl-"), "create printed the id alone: {id:?}");
    b.bl().arg("sync").assert().success();
    assert!(b.live_ids().contains(&id), "B sees A's created ball after sync: {:?}", b.live_ids());

    // A CLAIMS (materializes the delivery worktree) and commits a code change on it.
    let worktree = stdout(a.bl().args(["claim", &id, "--as", "alice"]).assert().success());
    std::fs::write(Path::new(&worktree).join("road.txt"), "paved\n").unwrap();
    git(Path::new(&worktree), &["add", "-A"]);
    git(Path::new(&worktree), &["commit", "-qm", &format!("pave [{id}]")]);

    // A CLOSES: close.pre squashes the delivery onto A's local main, the seal
    // archives the ball, close.post tears the worktree down and pushes the store.
    a.bl().args(["close", &id, "--as", "alice"]).assert().success();
    // The delivery squash is titled by the BALL (its title + the `[<id>]` tag),
    // not the WIP commit subject — the close names the delivery after the task.
    assert!(git_subject(&a.project, "main").contains(&format!("[{id}]")), "delivery squashed onto A's main");
    assert_eq!(std::fs::read_to_string(a.project.join("road.txt")).unwrap(), "paved\n", "the code change landed on A");
    assert!(!Path::new(&worktree).exists(), "close tore the worktree down");

    // B syncs again and SEES the close land — the ball leaves the live list.
    b.bl().arg("sync").assert().success();
    assert!(!b.live_ids().contains(&id), "B sees the close: ball archived out of the live list");
    assert!(a.live_ids().is_empty(), "A's own live list is empty too");

    // PRIME PRUNES THE SETTLED BRANCH: close deferred the `work/<id>` cleanup, so
    // the branch still stands right after the close — then `bl prime` (prime.post =
    // [bl-delivery, bl-tracker]) prunes it. The skill/prime.md promise, end to end.
    assert!(a.has_work_branch(&id), "close leaves the settled work/<id> branch for prime to reap");
    a.bl().arg("prime").assert().success();
    assert!(!a.has_work_branch(&id), "bl prime pruned the settled work/<id> branch");
}

#[test]
fn concurrent_creates_from_two_clones_both_land_after_the_lagging_side_syncs() {
    // Both developers create off one synced base. A publishes first, so B's
    // optimistic create.post push is rejected non-ff (§13 contention). The rejected
    // create leaves NO local leftover; B `bl sync`s the winner in and re-creates,
    // and after a final mutual sync BOTH balls stand in BOTH clones' live lists.
    let tmp = TempDir::new().unwrap();
    let (a, b) = two_devs(tmp.path());

    // A creates and wins the publish race to origin.
    let id_a = stdout(a.bl().args(["create", "Ship A", "--as", "alice"]).assert().success());

    // B, still on the pre-A base, creates too — its create.post push is rejected
    // because origin moved ahead; the documented recovery names `bl sync` + retry.
    b.bl()
        .args(["create", "Ship B", "--as", "bob"])
        .assert()
        .failure()
        .stderr(contains("push rejected: the remote store moved ahead").and(contains("run `bl sync`, then re-run")));

    // The rejected create rolled back cleanly: B carries no phantom ball, so the
    // recovery sync fast-forwards instead of hitting a self-inflicted divergence.
    assert!(b.live_ids().is_empty(), "rejected create left no local leftover: {:?}", b.live_ids());

    // B syncs A's ball in, then re-creates its own — now on top, the push lands.
    b.bl().arg("sync").assert().success();
    assert_eq!(b.live_ids(), vec![id_a.clone()], "B synced A's winner in");
    let id_b = stdout(b.bl().args(["create", "Ship B", "--as", "bob"]).assert().success());

    // Mutual sync: A pulls B's ball in. Both balls now stand in BOTH live lists —
    // the concurrent writes converged with no lost update.
    a.bl().arg("sync").assert().success();
    let mut a_ids = a.live_ids();
    let mut b_ids = b.live_ids();
    a_ids.sort();
    b_ids.sort();
    let mut want = vec![id_a, id_b];
    want.sort();
    assert_eq!(a_ids, want, "A sees both concurrent balls");
    assert_eq!(b_ids, want, "B sees both concurrent balls");
}

/// `git -C <repo> log -1 --format=%s <rev>` — a delivery/seed commit subject.
fn git_subject(repo: &Path, rev: &str) -> String {
    let out =
        std::process::Command::new("git").arg("-C").arg(repo).args(["log", "-1", "--format=%s", rev]).output().unwrap();
    assert!(out.status.success(), "git log failed in {}", repo.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}
