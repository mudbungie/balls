//! bl-4a88 regression, at the level it was REPORTED (bl-25e8): the full `bl`
//! workflow in a directory that was never `git init`ed. The src/ unit tests and
//! the `tests/delivery` harness drive the `bl-delivery` plugin binary directly;
//! only this test exercises the wiring + relay layer between them — that the
//! DEFAULT schedule wires `bl-delivery`, that balls relays its prime warning,
//! and that its claim/close abort surfaces as a nonzero `bl` exit in balls'
//! voice rather than a raw `fatal: not a git repository`.
//!
//! Freshly-built binaries, isolated HOME/XDG, a /tmp dir (the E2E convention).
//! `bl-delivery` is a DEFAULT plugin, so a plain `prime` binds it beside `bl` —
//! no manual symlink dance (cf. tests/chore_e2e.rs, which binds a non-default
//! plugin). tarpaulin counts src/ only, so this file is coverage-neutral.
//!
//! Self-guarding: were the dir accidentally a git repo, prime would not warn and
//! claim would not abort, so every assertion below would fail loudly.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// The raw git voice the bl-4a88 fix REPLACES — it must never reach the user.
const RAW_FATAL: &str = "fatal: not a git repository";

/// The isolated env every `bl` call needs, rooted at a NON-git invocation dir.
struct Env {
    home: PathBuf,
    state: PathBuf,
    config: PathBuf,
    dir: PathBuf,
    path: String,
}

/// Run `bl <args>` in the wired env. `BALLS_*` recursion bookkeeping is scrubbed:
/// this test runs INSIDE a `bl` invocation when the pre-commit gate fires under
/// `bl close`, and a top-level `bl` here must start at depth 0.
fn bl(e: &Env, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bl")
        .unwrap()
        .args(args)
        .env("HOME", &e.home)
        .env("XDG_STATE_HOME", &e.state)
        .env("XDG_CONFIG_HOME", &e.config)
        .env("PATH", &e.path)
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME")
        .current_dir(&e.dir)
        .output()
        .unwrap()
}

/// A NON-git invocation dir (deliberately never `git init`ed) + isolated env,
/// with the freshly-built binaries on PATH so `prime` binds the default plugins.
fn setup() -> (TempDir, Env) {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let dir = tmp.path().join("docs"); // a plain dir — NOT a git repo
    for d in [&home, &dir] {
        fs::create_dir_all(d).unwrap();
    }
    let bin_dir = Path::new(env!("CARGO_BIN_EXE_bl")).parent().unwrap();
    let e = Env {
        home,
        state: tmp.path().join("state"),
        config: tmp.path().join("config"),
        dir,
        path: format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap_or_default()),
    };
    (tmp, e)
}

#[test]
fn the_full_bl_workflow_in_a_non_git_dir_warns_then_aborts_cleanly() {
    let (_tmp, e) = setup();

    // STEP 2 — `bl prime --stealth` founds the substrate and SUCCEEDS (does not
    // refuse), but warns at the founding moment that delivery is unusable here,
    // naming BOTH drains, before any task is filed.
    let prime = bl(&e, &["prime", "--stealth", "--as", "e2e"]);
    let perr = String::from_utf8_lossy(&prime.stderr);
    assert!(prime.status.success(), "prime must not refuse a non-git dir: {perr}");
    assert!(perr.contains("is not a git repository"), "prime warning missing: {perr}");
    assert!(perr.contains("git init"), "the git-init drain must be named: {perr}");
    assert!(perr.contains("bl conf remove"), "the detach drain must be named: {perr}");
    assert!(!perr.contains(RAW_FATAL), "raw git voice leaked at prime: {perr}");

    // STEP 3 — create works: core task storage is its own XDG git substrate,
    // repo-independent.
    let create = bl(&e, &["create", "t", "--as", "e2e"]);
    assert!(create.status.success(), "create failed: {}", String::from_utf8_lossy(&create.stderr));
    let id = String::from_utf8_lossy(&create.stdout).trim().to_string();
    assert!(id.starts_with("bl-"), "expected a minted id, got {id:?}");

    // STEP 4 — claim ABORTS (nonzero), in balls' voice, NOT a raw git fatal: the
    // tracker can no longer mint a task you can never claim.
    let claim = bl(&e, &["claim", &id, "--as", "e2e"]);
    let cerr = String::from_utf8_lossy(&claim.stderr);
    assert!(!claim.status.success(), "claim should abort in a non-git dir: {cerr}");
    assert!(cerr.contains("is not a git repository"), "clean abort message missing: {cerr}");
    assert!(!cerr.contains(RAW_FATAL), "raw git voice leaked at claim: {cerr}");

    // STEP 5 — close is the ONLY retirement; it aborts cleanly too, so the
    // message (which names git init AND detaching delivery) is the way out.
    let close = bl(&e, &["close", &id, "--as", "e2e"]);
    let zerr = String::from_utf8_lossy(&close.stderr);
    assert!(!close.status.success(), "close should abort in a non-git dir: {zerr}");
    assert!(zerr.contains("is not a git repository"), "clean abort message missing: {zerr}");
    assert!(!zerr.contains(RAW_FATAL), "raw git voice leaked at close: {zerr}");
}
