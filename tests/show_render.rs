//! End-to-end render check for `bl show` (bl-66c6): drive the freshly-built
//! binary against throwaway, isolated stores and assert the LITERAL human field
//! block — the §9 projection the src unit tests exercise through `dispatch`, here
//! proven through the whole CLI → engine → git store → render round-trip.
//!
//! Five stores, each its own HOME/XDG (parallel-safe): a fully-populated live
//! ball (badge, tags, priority, `(on claim)`/`(on close)` blockers, children
//! rollup, claim-age line), a ball whose `-m` notes fold into the journal
//! oldest-first AFTER the body, a closed id (retirement badge + `retired` date),
//! an unknown id (`no such ball`), and a hand-corrupted store file (its parse
//! error, never a stale reconstruction). tarpaulin counts src/ only, so this
//! file is coverage-neutral.

use std::path::{Path, PathBuf};
use std::process::{Command as Sys, Output};

use assert_cmd::Command;
use tempfile::TempDir;

/// An isolated substrate: a real git project on `main` (so the delivery plugin
/// can fork `work/<id>` worktrees) plus the HOME/XDG that pin every clone into
/// the tempdir, never the real `$HOME`.
struct Env {
    home: PathBuf,
    state: PathBuf,
    project: PathBuf,
}

/// `git -C <cwd> <args>`, asserting success — project setup only.
fn git(cwd: &Path, args: &[&str]) {
    assert!(Sys::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success(), "git {args:?}");
}

/// A `bl` command wired to the isolated substrate. The inherited `BALLS_*`
/// recursion bookkeeping is scrubbed so a top-level `bl` here always starts at
/// depth 0 — the whole suite can run INSIDE a `bl close` pre-commit gate.
fn bl(e: &Env) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(&e.project)
        .env("HOME", &e.home)
        .env("XDG_STATE_HOME", &e.state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// Run `bl <args>`, assert success, return trimmed stdout (a verb's one product).
fn bl_ok(e: &Env, args: &[&str]) -> String {
    let out = bl(e).args(args).output().unwrap();
    assert!(out.status.success(), "bl {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `bl show <id> --plain` as a raw `Output` (human render; success not assumed —
/// the error scenarios read `stderr` + the exit code).
fn show(e: &Env, id: &str) -> Output {
    bl(e).args(["show", id, "--plain"]).output().unwrap()
}

/// `bl show <id> --plain`, asserting success, returning stdout verbatim.
fn show_ok(e: &Env, id: &str) -> String {
    let out = show(e, id);
    assert!(out.status.success(), "show {id} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Stand up an isolated, primed substrate (own HOME/XDG, a seeded git project).
fn setup() -> (TempDir, Env) {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "t"]);
    git(&project, &["config", "user.email", "t@e"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    let e = Env { home, state, project };
    bl_ok(&e, &["prime", "--as", "me"]);
    (tmp, e)
}

/// The single per-invocation clone bundle — its `tasks/tasks/<id>.md` is the
/// live store file the corruption scenario overwrites.
fn clone_root(e: &Env) -> PathBuf {
    std::fs::read_dir(e.state.join("balls/clones")).unwrap().next().unwrap().unwrap().path()
}

/// A fully-populated live ball renders every section with its literal label.
#[test]
fn show_renders_a_fully_populated_live_ball() {
    let (_tmp, e) = setup();
    // Two edge targets: a dependency (gates CLAIM) and a gate (gates CLOSE).
    let dep = bl_ok(&e, &["create", "Dep", "--as", "me"]);
    let gate = bl_ok(&e, &["create", "Gate", "--as", "me"]);
    // The ball under test carries both edges, a priority, and two tags.
    let m = bl_ok(
        &e,
        &["create", "Refactor big thing", "--needs", &dep, "--needs", &format!("{gate}:close"), "-p", "2", "-t",
          "infra", "-t", "refactor", "--body", "the design body", "--as", "me"],
    );
    let c1 = bl_ok(&e, &["create", "First child", "--parent", &m, "--as", "me"]);
    let c2 = bl_ok(&e, &["create", "Second child", "--parent", &m, "--as", "me"]);

    // Close the CLAIM-gating dep so its file is gone ⇒ the blocker resolves and
    // `m` becomes claimable — the edge still shows, a dead blocker never blocks.
    bl_ok(&e, &["claim", &dep, "--as", "me"]);
    bl_ok(&e, &["close", &dep, "--as", "me"]);
    bl_ok(&e, &["claim", &m, "--as", "alice"]);

    let out = show_ok(&e, &m);
    // Header badge + status: a live claimed ball reads `claimed` both places.
    assert!(out.contains(&format!("claimed  {m}  Refactor big thing")), "header badge:\n{out}");
    assert!(out.contains("status   claimed"), "status:\n{out}");
    // Claimant + the DERIVED claim-age line hung under it (bl-46ef); the claim
    // and this render are seconds apart, so the coarse age floors at `0m`.
    assert!(out.contains("claimant alice"), "claimant:\n{out}");
    assert!(out.contains("(0m ago)"), "claim-age line:\n{out}");
    assert!(out.contains("priority 2"), "priority:\n{out}");
    assert!(out.contains("tags     infra, refactor"), "tags:\n{out}");
    // Blockers annotated by the transition each gates — insertion order (dep, gate).
    assert!(
        out.contains(&format!("  blockers\n    {dep} (on claim)\n    {gate} (on close)\n")),
        "annotated blockers:\n{out}"
    );
    // Children rollup: each with its OWN status badge (catalog/id order, not
    // creation order — assert each line independently).
    assert!(out.contains("  children\n"), "children header:\n{out}");
    assert!(out.contains(&format!("ready    {c1}  First child")), "first child badge line:\n{out}");
    assert!(out.contains(&format!("ready    {c2}  Second child")), "second child badge line:\n{out}");
    assert!(out.contains("\nthe design body"), "body:\n{out}");
}

/// `-m` notes ride mutating commits and fold into the journal oldest-first,
/// each indented under its op line, in one paragraph AFTER the body.
#[test]
fn show_folds_journal_notes_oldest_first_after_the_body() {
    let (_tmp, e) = setup();
    let j = bl_ok(&e, &["create", "Journaled", "--body", "body text here", "--as", "scribe"]);
    // A bare `-m` seals nothing (no field delta); the note rides a real edit.
    bl_ok(&e, &["update", &j, "-p", "1", "-m", "first journal note", "--as", "scribe"]);
    bl_ok(&e, &["update", &j, "-p", "2", "-m", "second journal note", "--as", "scribe"]);

    let out = show_ok(&e, &j);
    // The notes appear, each indented under its `update` op line.
    assert!(out.contains("  journal\n"), "journal header:\n{out}");
    assert!(out.contains("      first journal note\n"), "first note indented:\n{out}");
    assert!(out.contains("      second journal note\n"), "second note indented:\n{out}");
    // Oldest-first: the create entry precedes both updates, and the body
    // precedes the whole journal paragraph.
    let body = out.find("body text here").expect("body present");
    let journal = out.find("  journal\n").expect("journal present");
    let first = out.find("first journal note").expect("first note present");
    let second = out.find("second journal note").expect("second note present");
    assert!(body < journal, "journal folds after the body:\n{out}");
    assert!(first < second, "notes render oldest-first:\n{out}");
}

/// A closed id resolves from history and renders the retirement badge with a
/// `retired` date in place of the live status.
#[test]
fn show_renders_a_closed_id_with_retirement_badge_and_date() {
    let (_tmp, e) = setup();
    let c = bl_ok(&e, &["create", "To be closed", "--as", "me"]);
    bl_ok(&e, &["claim", &c, "--as", "me"]);
    bl_ok(&e, &["close", &c, "--as", "me"]);

    let out = show_ok(&e, &c);
    assert!(out.contains(&format!("closed   {c}  To be closed")), "retirement badge:\n{out}");
    assert!(out.contains("status   closed"), "closed status:\n{out}");
    assert!(out.contains("retired  20"), "retired date line:\n{out}"); // ISO year prefix
    // The journal still renders — its close commit is the last entry.
    assert!(out.contains("close    me\n"), "close op in journal:\n{out}");
}

/// An id that resolves to neither a live nor a dead ball is an error naming it.
#[test]
fn show_errors_no_such_ball_for_an_unknown_id() {
    let (_tmp, e) = setup();
    let out = show(&e, "bl-9999");
    assert!(!out.status.success(), "unknown id must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no such ball: bl-9999"), "names the missing id: {err}");
}

/// A corrupt store file surfaces its parse error — not `no such ball`, and above
/// all not the history fallthrough resurrecting a stale incarnation (bl-528c).
#[test]
fn show_surfaces_a_parse_error_for_a_corrupt_ball() {
    let (_tmp, e) = setup();
    let id = bl_ok(&e, &["create", "Original title", "--body", "real body", "--as", "me"]);
    // Overwrite the LIVE store file with invalid frontmatter (a non-string title).
    let file = clone_root(&e).join("tasks").join("tasks").join(format!("{id}.md"));
    std::fs::write(&file, "+++\ntitle = 1\n+++\n").unwrap();

    let out = show(&e, &id);
    assert!(!out.status.success(), "corrupt ball must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(&format!("tasks/{id}.md")), "names the file: {err}");
    assert!(err.contains("invalid frontmatter"), "carries the parse error: {err}");
    assert!(!err.contains("Original title"), "no stale resurrection: {err}");
}
