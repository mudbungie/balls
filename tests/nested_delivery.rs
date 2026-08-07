//! bl-7b71 end-to-end through the real `bl` binary: composition — an epic that
//! is a REF, not a label on a report.
//!
//! A ball that close-gates its live parent (`--parent E --blocks close`) has
//! that parent as its delivery TARGET: `claim` forks `work/<E>` (minted at the
//! integration head on the first child, no worktree, nothing to orphan), `close`
//! requires `work/<E>` to be incorporated already (bl-a1a4) and squashes back
//! onto it. `main` does not move until the
//! epic itself closes — parentless, so ITS target is the integration branch —
//! and then the accumulated children land as ONE commit.
//!
//! The negative half matters as much: a bare `--parent` is containment only and
//! keeps delivering flat to `main`, so nesting needs BOTH coordinates.
//!
//! tarpaulin counts src/ only, so this integration file is coverage-neutral; the
//! pure derivation is unit-tested in `src/target_tests.rs` and the plugin-seam
//! matrix in `src/delivery_tests.rs`.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// `bl` rooted in `project`, HOME/`XDG_STATE_HOME` pinned under the tempdir. The
/// inherited `BALLS_*` recursion bookkeeping is scrubbed — this file itself can
/// run inside a `bl close` gate, so a top-level `bl` here must start at depth 0.
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

/// `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// `git -C <cwd> <args>` capturing trimmed stdout.
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed in {}", cwd.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// Does `refs/heads/work/<id>` exist in `root`? (`rev-parse --verify --quiet`
/// is the exit-code form — no output to parse, no `fatal` on stderr.)
fn work_branch_exists(root: &Path, id: &str) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/work/{id}")])
        .status()
        .unwrap()
        .success()
}

/// A project repo seeded on `main` plus a primed stealth store, both under `tmp`.
fn project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state, root) = (tmp.join("h"), tmp.join("s"), tmp.join("proj"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&state).unwrap();
    git(tmp, &["init", "-q", "-b", "main", &root.to_string_lossy()]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "user.email", "t@t"]);
    fs::write(root.join("seed.txt"), "seed\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "seed"]);
    bl(&root, &home, &state).arg("prime").assert().success();
    (root, home, state)
}

/// Claim `id`, commit `file` in the returned worktree, close it.
fn work_and_close(root: &Path, home: &Path, state: &Path, id: &str, file: &str) -> PathBuf {
    let wt = PathBuf::from(stdout(bl(root, home, state).args(["claim", id, "--as", "me"]).assert().success()));
    fs::write(wt.join(file), "done\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", &format!("work [{id}]")]);
    bl(root, home, state).args(["close", id, "--as", "me"]).assert().success();
    wt
}

#[test]
fn children_accumulate_on_the_epics_ref_and_the_epic_lands_them_all_at_once() {
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let create = |args: &[&str]| stdout(bl(&root, &home, &state).args(args).assert().success());

    let epic = create(&["create", "The epic", "--as", "me"]);
    // BOTH coordinates: containment (`--parent`) plus the close-gate that IS the
    // nesting declaration (a bare `--blocks close` gates the parent).
    let kid = create(&["create", "Kid", "--parent", &epic, "--blocks", "close", "--as", "me"]);
    let sib = create(&["create", "Sib", "--parent", &epic, "--blocks", "close", "--as", "me"]);

    // The first child's claim mints `work/<epic>` at main and forks it; its close
    // squashes back onto that ref. main NEVER moves.
    work_and_close(&root, &home, &state, &kid, "kid.txt");
    let epic_branch = format!("work/{epic}");
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", &epic_branch]), format!("Kid [{kid}]"));
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), "seed", "a child delivers, it does not land");

    // The second child forks the SAME ref — it sees its sibling's delivered work,
    // which is the whole point of composition (today each ball squashes to main
    // independently, so siblings never see each other).
    let sib_wt = PathBuf::from(stdout(bl(&root, &home, &state).args(["claim", &sib, "--as", "me"]).assert().success()));
    assert!(sib_wt.join("kid.txt").exists(), "the sibling forked the epic's ref, not clean main");
    fs::write(sib_wt.join("sib.txt"), "done\n").unwrap();
    git(&sib_wt, &["add", "-A"]);
    git(&sib_wt, &["commit", "-qm", &format!("work [{sib}]")]);
    bl(&root, &home, &state).args(["close", &sib, "--as", "me"]).assert().success();
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", &epic_branch]), format!("Sib [{sib}]"));
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), "seed");

    // Both gates are resolved, so the epic — parentless, target = main — closes
    // and lands the accumulated work as ONE commit: one reviewable unit.
    bl(&root, &home, &state).args(["close", &epic, "--as", "me"]).assert().success();
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), format!("The epic [{epic}]"));
    assert_eq!(git_out(&root, &["show", "main:kid.txt"]), "done");
    assert_eq!(git_out(&root, &["show", "main:sib.txt"]), "done");
    let subjects = git_out(&root, &["log", "--format=%s", "main"]);
    assert_eq!(subjects.lines().count(), 2, "one squash on top of the seed: {subjects}");
}

#[test]
fn a_sibling_that_forked_first_refuses_until_it_incorporates_the_epic() {
    // bl-a1a4 at depth. AA and AB both fork `work/<epic>` BEFORE either closes.
    // AA closes first, so AB's TARGET has moved — on a file AB never touched,
    // which git would fold in without a murmur. The rule is fractal: `child ->
    // work/<parent>` is the same delivery edge as `root -> integration`, and a
    // stale source refuses at both. Reconciliation is AB's, in AB's worktree.
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let create = |args: &[&str]| stdout(bl(&root, &home, &state).args(args).assert().success());
    let epic = create(&["create", "The epic", "--as", "me"]);
    let aa = create(&["create", "AA", "--parent", &epic, "--blocks", "close", "--as", "me"]);
    let ab = create(&["create", "AB", "--parent", &epic, "--blocks", "close", "--as", "me"]);

    let claim = |id: &str| PathBuf::from(stdout(bl(&root, &home, &state).args(["claim", id, "--as", "me"]).assert().success()));
    let (first, second) = (claim(&aa), claim(&ab)); // both fork the epic's ref first
    for (wt, file, id) in [(&first, "aa.txt", &aa), (&second, "ab.txt", &ab)] {
        fs::write(wt.join(file), "done\n").unwrap();
        git(wt, &["add", "-A"]);
        git(wt, &["commit", "-qm", &format!("work [{id}]")]);
    }
    bl(&root, &home, &state).args(["close", &aa, "--as", "me"]).assert().success();

    let epic_branch = format!("work/{epic}");
    bl(&root, &home, &state)
        .args(["close", &ab, "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("stale source"))
        .stderr(contains(format!("Merge or rebase {epic_branch} into the work/{ab} worktree")));
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", &epic_branch]), format!("AA [{aa}]"), "the refusal moved nothing");

    // AB incorporates its target and closes; the epic accumulates both.
    git(&second, &["merge", "-q", "--no-edit", &epic_branch]);
    bl(&root, &home, &state).args(["close", &ab, "--as", "me"]).assert().success();
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", &epic_branch]), format!("AB [{ab}]"));
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), "seed");

    // And the epic closes normally: main never moved under it, so it has nothing
    // to reconcile and lands both children as ONE commit.
    bl(&root, &home, &state).args(["close", &epic, "--as", "me"]).assert().success();
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), format!("The epic [{epic}]"));
    assert_eq!(git_out(&root, &["show", "main:aa.txt"]), "done");
    assert_eq!(git_out(&root, &["show", "main:ab.txt"]), "done");
}

#[test]
fn a_bare_parent_gates_nothing_and_keeps_delivering_flat_to_main() {
    // Containment alone is a display-only pointer — the per-child offramp that
    // deletes no config when unused. Without the close-gate the child lands on
    // main exactly as it always did, and the parent's ref is never even minted.
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let create = |args: &[&str]| stdout(bl(&root, &home, &state).args(args).assert().success());

    let epic = create(&["create", "The epic", "--as", "me"]);
    let kid = create(&["create", "Kid", "--parent", &epic, "--as", "me"]);
    work_and_close(&root, &home, &state, &kid, "kid.txt");

    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), format!("Kid [{kid}]"));
    assert!(!work_branch_exists(&root, &epic), "no nesting ⇒ no target ref is ever minted");
}

#[test]
fn a_closed_balls_own_branch_dies_with_its_close_at_every_depth() {
    // bl-ce3b. Branch deletion used to be DEFERRED to `prime`, which decides by
    // scanning the INTEGRATION branch for a `[bl-<id>]` delivery commit. A
    // NESTED child never puts one there — it delivers into `work/<parent>` —
    // so prime read it as "undelivered" and correctly refused to delete it,
    // forever: one permanently leaked branch per closed child. The close itself
    // KNOWS it just delivered, so it deletes its own branch and the nested case
    // stops needing the reconstruction at all.
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let create = |args: &[&str]| stdout(bl(&root, &home, &state).args(args).assert().success());
    let epic = create(&["create", "The epic", "--as", "me"]);
    let kid = create(&["create", "Kid", "--parent", &epic, "--blocks", "close", "--as", "me"]);

    // THE LEAK CASE: the child's work is on `work/<epic>`, and NOTHING on main
    // carries the child's id — the exact state prime cannot classify.
    work_and_close(&root, &home, &state, &kid, "kid.txt");
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), "seed", "the child delivered, it did not land");
    assert!(work_branch_exists(&root, &epic), "the epic's ref is live — it holds the child's delivered work");
    assert!(!work_branch_exists(&root, &kid), "the child's close deleted its own branch");

    // And prime — the old cleanup site — has nothing left to report about it.
    let primed = bl(&root, &home, &state).arg("prime").assert().success();
    let noise = String::from_utf8(primed.get_output().stderr.clone()).unwrap();
    assert!(!noise.contains(&kid), "no debris line survives for the closed child:\n{noise}");

    // Same act at the root: the epic's own close lands on main and takes
    // `work/<epic>` with it, so the ref that held the children is gone too.
    bl(&root, &home, &state).args(["close", &epic, "--as", "me"]).assert().success();
    assert_eq!(git_out(&root, &["show", "main:kid.txt"]), "done");
    assert!(!work_branch_exists(&root, &epic), "the epic's close deleted its own branch");
}

#[test]
fn the_rendered_column_says_delivered_here_until_the_target_itself_lands() {
    // bl-6915: a closed child is DELIVERED to its target, not LANDED on main,
    // and absence is a closed ball's whole record. The fix is a rendered column,
    // not a stored field — and the column IS the landed-vs-delivered marker: it
    // derives only against a LIVE target, so it vanishes the moment the target
    // lands. No git query, no schema growth.
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let create = |args: &[&str]| stdout(bl(&root, &home, &state).args(args).assert().success());
    let epic = create(&["create", "The epic", "--as", "me"]);
    let kid = create(&["create", "Kid", "--parent", &epic, "--blocks", "close", "--as", "me"]);

    // Live and open: where this ball's work is GOING.
    let live = stdout(bl(&root, &home, &state).args(["list", "--plain"]).assert().success());
    assert!(live.contains(&format!("{kid}  Kid  ->{epic}")), "open ball shows its target:\n{live}");
    assert!(!live.contains(&format!("{epic}  The epic  ->")), "a parentless epic delivers flat:\n{live}");

    // Closed into the epic: the marker is now "delivered, not landed" — the ball
    // is otherwise invisible, which is exactly the cost this column pays off.
    work_and_close(&root, &home, &state, &kid, "kid.txt");
    let dead = stdout(bl(&root, &home, &state).args(["list", "--all", "--plain"]).assert().success());
    assert!(dead.contains(&format!("closed   {kid}  Kid")) && dead.contains(&format!("->{epic}")),
        "closed child still names its target:\n{dead}");
    let shown = stdout(bl(&root, &home, &state).args(["show", &kid, "--plain"]).assert().success());
    assert!(shown.contains(&format!("delivers {epic}")), "show carries the same derived fact:\n{shown}");

    // `--json` is the lossless bedrock schema record and does NOT grow (§9).
    let json = stdout(bl(&root, &home, &state).args(["list", "--all", "--json"]).assert().success());
    assert!(!json.contains("delivers") && !json.contains("\"target\""), "projection only:\n{json}");

    // The epic lands, and with it the child's work: the target is no longer
    // live, so the marker is gone. Landed is the unmarked state, as it always was.
    bl(&root, &home, &state).args(["close", &epic, "--as", "me"]).assert().success();
    assert_eq!(git_out(&root, &["show", "main:kid.txt"]), "done");
    let landed = stdout(bl(&root, &home, &state).args(["list", "--all", "--plain"]).assert().success());
    assert!(!landed.contains("->"), "a landed ball carries no target marker:\n{landed}");
}
