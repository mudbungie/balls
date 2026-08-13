//! End-to-end drive of the §10 edge-authoring FRONT DOOR (bl-908a): the real
//! `bl` binary, in isolated tempdirs on throwaway `main` repos, exercises every
//! way a blocker edge is spelled at the CLI and every refusal that guards it.
//!
//! What the src/ unit tests (fake stores, hand-built [`Blocker`]s) cannot reach,
//! this file asserts as OBSERVABLE outcomes of the shipped binary: `show --json`
//! carries the RECIPROCAL edges `--subtask-of`/`--blocks` mint on OTHER balls
//! (parent + `on=close` for the subtask, likewise for `--blocks close`); the
//! bl-54fe write-time acyclicity refusal names the full claim/close loop and
//! exits nonzero; the edge-target liveness refusal DISTINGUISHES a never-minted
//! id ("not a known id") from an already-closed one ("already closed"); and a
//! generic non-lifecycle gate (`--blocks ID:update`) actually refuses the real
//! `bl update` through core's op-keyed guard (§10/§15) — no per-op carve-out.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir so its clone bundle never touches the real `$HOME`; the shipped
/// plugins resolve beside the built `bl`.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        // bl-1266: a leaked depth makes the tracker read this shelled `bl` as NESTED
        // and skip its push — the suite runs inside the close hook's plugin chain.
        .env_remove("BALLS_PLUGIN_DEPTH");
    cmd
}

/// `git -C <cwd> <args>`, asserting success (harness setup with plain git).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A real project repo on `main` with a seed commit, plus a primed stealth
/// landing (so the delivery plugin can fork `work/<id>` worktrees for close).
fn primed_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// A verb's one stdout product (create's id / claim's worktree path), trimmed.
fn stdout(a: assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// `bl create "<title>" <extra…>` → the minted id (printed alone to stdout, §9).
fn create(project: &Path, home: &Path, state: &Path, title: &str, extra: &[&str]) -> String {
    let mut args = vec!["create", title];
    args.extend_from_slice(extra);
    args.push("--as");
    args.push("me");
    stdout(bl(project, home, state).args(&args).assert().success())
}

/// `bl show <id> --json` parsed to the bedrock frontmatter mirror.
fn show(project: &Path, home: &Path, state: &Path, id: &str) -> Value {
    serde_json::from_str(&stdout(bl(project, home, state).args(["show", id, "--json"]).assert().success())).unwrap()
}

/// Does `task`'s `blockers` carry the edge `{id, on}`? (the reciprocal an edge
/// flag mints lands on the TARGET ball, not the authored one.)
fn has_edge(task: &Value, id: &str, on: &str) -> bool {
    task["blockers"].as_array().unwrap().iter().any(|b| b["id"] == id && b["on"] == on)
}

#[test]
fn subtask_of_and_blocks_wire_the_reciprocal_edges_on_the_target() {
    // The whole point of the front door: containment (`--parent`) mints NO edge,
    // but `--subtask-of`/`--blocks` DO — and the edge lands on the OTHER ball.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    // `--subtask-of E` = `--parent E --blocks close` in one word: the child's
    // parent points at E, and E gains an `on=close` gate naming the child — the
    // two coordinates of §11 nesting (E cannot retire until the child closes,
    // and the child delivers into E's ref).
    let epic = create(&project, &home, &state, "Epic E", &[]);
    let child = create(&project, &home, &state, "Subtask", &["--subtask-of", &epic]);
    assert_eq!(show(&project, &home, &state, &child)["parent"], epic.as_str(), "child's parent is the epic");
    assert!(has_edge(&show(&project, &home, &state, &epic), &child, "close"), "epic close-gated on the child");

    // `--parent P --blocks close`: the bare `--blocks OP` gates the parent P's
    // `close` on the new gate G — the reciprocal `on=close` edge lands on P.
    let parent = create(&project, &home, &state, "Parent P", &[]);
    let gate = create(&project, &home, &state, "Close gate", &["--parent", &parent, "--blocks", "close"]);
    assert_eq!(show(&project, &home, &state, &gate)["parent"], parent.as_str(), "gate's parent is P");
    assert!(has_edge(&show(&project, &home, &state, &parent), &gate, "close"), "P close-gated on the gate");

    // `--blocks ID:OP` gates a NON-parent by id: X close-gated on the new ball Y.
    let ball_x = create(&project, &home, &state, "X", &[]);
    let ball_y = create(&project, &home, &state, "Y", &["--blocks", &format!("{ball_x}:close")]);
    assert!(has_edge(&show(&project, &home, &state, &ball_x), &ball_y, "close"), "X close-gated on Y via ID:OP");
    // Containment never leaks a gate the other way: Y stays edge-free.
    assert!(show(&project, &home, &state, &ball_y)["blockers"].as_array().unwrap().is_empty(), "Y carries no edge");
}

#[test]
fn a_write_time_claim_close_cycle_is_refused_naming_the_full_loop() {
    // bl-54fe: the classic mis-wiring is a verification gate spelled BOTH ways.
    // `--parent P --blocks close` makes P wait on G's close; then `--needs P`
    // makes G wait on P's claim — a claim/close loop no ordering resolves. The
    // add is refused at WRITE time (not silently at `bl close`), naming the loop.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    let parent = create(&project, &home, &state, "Parent P", &[]);
    let gate = create(&project, &home, &state, "Gate G", &["--parent", &parent, "--blocks", "close"]);

    // G -claim-> P (the new `--needs` edge) -close-> G (the pre-existing gate).
    bl(&project, &home, &state)
        .args(["update", &gate, "--needs", &parent, "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("closes a deadlock").and(contains(format!("{gate} -claim-> {parent} -close-> {gate}"))));

    // The refusal aborted the op cleanly: G never gained the cyclic edge.
    assert!(show(&project, &home, &state, &gate)["blockers"].as_array().unwrap().is_empty(), "the cyclic edge was not sealed");

    // The unlink escape hatch is never refused, even of an edge that never
    // landed — `--no-needs` is the in-band recovery, so it always passes.
    bl(&project, &home, &state).args(["update", &gate, "--no-needs", &parent, "--as", "me"]).assert().success();
}

#[test]
fn edge_target_liveness_refusals_distinguish_unknown_from_already_closed() {
    // Every edge target must be LIVE. The two dead shapes get DIFFERENT names:
    // a never-minted id is a typo/hallucination ("not a known id"); an
    // already-closed one is an edge born resolved ("already closed").
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    // Unknown: `bl-nope` was never minted, so it is not a known id.
    bl(&project, &home, &state)
        .args(["create", "needs a ghost", "--needs", "bl-nope", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("'bl-nope' is not a known id"));

    // Closed: mint a ball, deliver it end-to-end (claim → commit → close), then
    // aim a fresh `--needs` at its retired id — a dead blocker can never block.
    let dead = create(&project, &home, &state, "Doomed", &[]);
    let worktree = stdout(bl(&project, &home, &state).args(["claim", &dead, "--as", "me"]).assert().success());
    std::fs::write(Path::new(&worktree).join("f.txt"), "done\n").unwrap();
    git(Path::new(&worktree), &["add", "-A"]);
    git(Path::new(&worktree), &["commit", "-qm", &format!("work [{dead}]")]);
    bl(&project, &home, &state).args(["close", &dead, "--as", "me"]).assert().success();

    bl(&project, &home, &state)
        .args(["create", "needs a corpse", "--needs", &dead, "--as", "me"])
        .assert()
        .failure()
        .stderr(contains(format!("'{dead}' is already closed")));
}

#[test]
fn a_generic_non_lifecycle_gate_refuses_the_real_bl_update() {
    // `on` is ANY op (§10/§15) — not just claim/close. `--blocks ID:update`
    // gates the TARGET's `update`, and core's single op-keyed guard refuses the
    // real `bl update` while the blocker is live. No per-op carve-out, no plugin.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    let target = create(&project, &home, &state, "Frozen until A", &[]);
    let blocker = create(&project, &home, &state, "Blocker A", &["--blocks", &format!("{target}:update")]);
    assert!(has_edge(&show(&project, &home, &state, &target), &blocker, "update"), "target update-gated on A");

    // A non-lifecycle edge is NOT a cycle candidate, so the reciprocal sealed
    // fine — but the very verb it names is now refused by core's `enforce::gate`.
    bl(&project, &home, &state)
        .args(["update", &target, "--title", "sneak", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("update:").and(contains(&*target)).and(contains("blocked by unresolved")).and(contains(&*blocker)));

    // The refusal held: the title never changed.
    assert_eq!(show(&project, &home, &state, &target)["title"], "Frozen until A");
}
