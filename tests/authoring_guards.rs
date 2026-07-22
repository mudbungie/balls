//! End-to-end lock on the §9 AUTHORING guard rails — the misuse envelope an LLM
//! agent actually hits — driven through the real `bl` binary in isolated tempdirs
//! on throwaway `main` repos (never the dev repo's own task list). The src/
//! unit tests reach these guards with fake `Flags`; this file asserts them as
//! OBSERVABLE binary behavior AND pins the invariant the wording only promises:
//! every refusal aborts BEFORE any store write, so `bl list --json` and the
//! on-disk task-file set stay byte-identical across the failed op.
//!
//! Findings (2026-07-19): every guard HOLDS — each refusal exits nonzero with the
//! documented wording and leaves the store pristine; `--edit` under piped stdin
//! fails fast (never hangs); a `--`-shelled `-`-leading title is taken byte-exact.
//! No `// FINDING:` pins were needed.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir so its clone bundle never touches the real `$HOME`, and any inherited
/// plugin-chain env scrubbed (a run from inside the close-hook chain must not
/// leak depth/name). Shipped plugins resolve beside the built `bl`.
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

/// A verb's one stdout product (create's id / claim's worktree path), trimmed.
fn stdout(a: assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
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

/// `bl create "<title>" <extra…> --as me` → the minted id (printed alone, §9).
fn create(project: &Path, home: &Path, state: &Path, title: &str, extra: &[&str]) -> String {
    let mut args = vec!["create", title];
    args.extend_from_slice(extra);
    args.extend(["--as", "me"]);
    stdout(bl(project, home, state).args(&args).assert().success())
}

/// The store's observable state: the `list --json` value paired with the count of
/// on-disk `tasks/*.md` files — the pair a refusal must leave byte-identical (no
/// silent mutation, no orphan task file).
fn snapshot(project: &Path, home: &Path, state: &Path) -> (Value, usize) {
    let raw = stdout(bl(project, home, state).args(["list", "--json"]).assert().success());
    let list = serde_json::from_str(if raw.is_empty() { "[]" } else { &raw }).unwrap();
    let tasks = balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
        .clone_dir(project)
        .store()
        .join("tasks");
    let files = std::fs::read_dir(&tasks).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some()).count();
    (list, files)
}

/// `bl show <id> --json` → the bedrock frontmatter mirror.
fn show(project: &Path, home: &Path, state: &Path, id: &str) -> Value {
    serde_json::from_str(&stdout(bl(project, home, state).args(["show", id, "--json"]).assert().success())).unwrap()
}

#[test]
fn create_only_and_occupancy_flags_are_refused_and_never_mutate_the_store() {
    // The bulk of the misuse envelope: create-only flags aimed at `update`, a
    // bare `--blocks OP` with no home, an unknown op token, field edits smuggled
    // onto the occupancy verbs, and an edge at a never-minted id. Each is one
    // refusal against a shared two-ball store; because a guard aborts pre-write,
    // ONE baseline snapshot stays valid across every case.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let a = create(&project, &home, &state, "Ball A", &[]);
    let b = create(&project, &home, &state, "Ball B", &[]);
    let baseline = snapshot(&project, &home, &state);

    let cases: [(Vec<String>, &str); 7] = [
        // §9: `update` edits this task's OWN fields; the two reciprocal edges an
        // edge-on-ANOTHER-task would mint stay create-only, refused by name.
        (vec!["update".into(), a.clone(), "--subtask-of".into(), b.clone()], "--subtask-of carries a reciprocal close-gate"),
        (vec!["update".into(), a.clone(), "--blocks".into(), "close".into()], "--blocks (a reciprocal edge on ANOTHER task) is create-only"),
        // A bare `--blocks OP` has no target without a parent/subtask-of home.
        (vec!["create".into(), "gate".into(), "--blocks".into(), "close".into()], "--blocks OP needs --parent/--subtask-of"),
        // `on` is ANY op, but must be a KNOWN op — a typo'd verb is named.
        (vec!["create".into(), "x".into(), "--needs".into(), format!("{b}:bogus_op")], "'bogus_op' is not a known op"),
        // The occupancy/retire verbs shape no ball fields: every field edit refused.
        (vec!["claim".into(), a.clone(), "--title".into(), "sneak".into()], "claim: takes no field edits"),
        (vec!["close".into(), a.clone(), "-p".into(), "1".into()], "close: takes no field edits"),
        // Edge-target liveness for `--subtask-of` (only `--needs` is e2e-tested
        // elsewhere): a never-minted target is a typo/hallucination.
        (vec!["create".into(), "c".into(), "--subtask-of".into(), "bl-nope".into()], "'bl-nope' is not a known id"),
    ];

    for (args, wording) in cases {
        let full: Vec<&str> = args.iter().map(String::as_str).chain(["--as", "me"]).collect();
        bl(&project, &home, &state).args(&full).assert().failure().stderr(contains(wording));
        assert_eq!(snapshot(&project, &home, &state), baseline, "refusal `{args:?}` must not mutate the store");
    }
}

#[test]
fn a_dashdash_shelled_title_is_taken_byte_exactly() {
    // The documented untrusted-input pattern (`bl create -- "$TITLE"`, §9): `--`
    // ends option parsing, so a `-`-leading title can't hijack a flag — it is
    // stored byte-for-byte, verified through the lossless `show --json` mirror.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "--as", "me", "--", "-p not-a-flag"]).assert().success());
    assert_eq!(show(&project, &home, &state, &id)["title"], "-p not-a-flag");
}

#[test]
fn edge_targets_that_are_already_closed_are_refused_by_both_edge_flags() {
    // The dead-target half of liveness for `--subtask-of`/`--blocks` (only
    // `--needs` is e2e-tested elsewhere): an already-closed id is an edge born
    // resolved — a dead blocker can never block — refused, naming the corpse. A
    // real closed ball is built the only honest way: claim → commit → close.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    let dead = create(&project, &home, &state, "Doomed", &[]);
    let worktree = stdout(bl(&project, &home, &state).args(["claim", &dead, "--as", "me"]).assert().success());
    std::fs::write(Path::new(&worktree).join("f.txt"), "done\n").unwrap();
    git(Path::new(&worktree), &["add", "-A"]);
    git(Path::new(&worktree), &["commit", "-qm", &format!("work [{dead}]")]);
    bl(&project, &home, &state).args(["close", &dead, "--as", "me"]).assert().success();
    let baseline = snapshot(&project, &home, &state);

    for edge in [vec!["--subtask-of".to_string(), dead.clone()], vec!["--blocks".to_string(), format!("{dead}:close")]] {
        let full: Vec<&str> = ["create", "gate"].into_iter().chain(edge.iter().map(String::as_str)).chain(["--as", "me"]).collect();
        bl(&project, &home, &state)
            .args(&full)
            .assert()
            .failure()
            .stderr(contains(format!("'{dead}' is already closed")));
        assert_eq!(snapshot(&project, &home, &state), baseline, "a dead-edge refusal must not mutate the store");
    }
}

#[test]
fn a_single_call_claim_cycle_is_refused_leaving_no_orphan_task() {
    // The classic mis-wiring in ONE call: `--subtask-of E` close-gates E on the
    // new ball, and `--needs E` claim-gates the new ball on E — a loop no
    // claim/close order resolves (bl-54fe). The write-time acyclicity guard names
    // the full loop and aborts before minting anything: no `tasks/<id>.md` orphan.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let epic = create(&project, &home, &state, "Epic", &[]);
    let baseline = snapshot(&project, &home, &state);

    bl(&project, &home, &state)
        .args(["create", "verify", "--subtask-of", &epic, "--needs", &epic, "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("closes a deadlock").and(contains("no claim/close order resolves that loop")));

    // The refused create minted nothing — the task-file set and list are unchanged.
    assert_eq!(snapshot(&project, &home, &state), baseline, "the cyclic create left no orphan task");
}

#[test]
fn edit_fails_fast_under_piped_stdin_and_excludes_the_field_flags() {
    // `--edit` is the human projection: it opens `$EDITOR` on the stored buffer,
    // so a non-tty stdin is an ERROR, not a hang — agents keep using flags. The
    // `.timeout` BOUNDS a regression to a hang; the assertion demands the fast,
    // named refusal instead. And `--edit` + a field flag would race over the
    // payload, so they are a clean either/or, refused mutually-exclusive.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let id = create(&project, &home, &state, "Editable", &[]);
    let baseline = snapshot(&project, &home, &state);

    bl(&project, &home, &state)
        .args(["update", &id, "--edit", "--as", "me"])
        .write_stdin("")
        .timeout(Duration::from_secs(15))
        .assert()
        .failure()
        .stderr(contains("stdin is not a tty"));

    bl(&project, &home, &state)
        .args(["update", &id, "--edit", "--title", "X", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("--edit and the field flags are mutually exclusive"));

    assert_eq!(snapshot(&project, &home, &state), baseline, "neither --edit refusal mutated the store");
}
