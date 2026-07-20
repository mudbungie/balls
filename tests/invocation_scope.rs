//! "Where am I?" — the substrate a `bl` op addresses is keyed on the LITERAL
//! current directory, with NO git-root discovery anywhere in `src/`. `main`
//! passes `env::current_dir()` straight through as `invocation_path`
//! (`src/main.rs`), and `Xdg::clone_dir` percent-encodes that path into the
//! SINGLE component that keys the whole clone bundle — landing + store
//! (`src/layout.rs`). There is no `git rev-parse --show-toplevel`; the project
//! root is never computed. So the same project, addressed from two different
//! working directories, resolves to two different task stores.
//!
//! These end-to-end tests pin what that cwd-keying actually DOES, through the
//! freshly-built binary on throwaway fixtures with isolated HOME/XDG — never the
//! dev repo's own store. The `// FINDING:` comments record the architecture
//! footgun each test proves; the tests themselves pin current reality.
//!
//! FINDING SUMMARY (classification: ARCHITECTURAL):
//!   - A READ (`list`) from a subdirectory is a SILENT empty success — it
//!     addresses a store that was never founded, indistinguishable from "this
//!     project has no tasks" (test 1).
//!   - A MUTATION (`create`) from a subdirectory is GUARDED: it refuses with
//!     "no balls checkout here" rather than silently founding a sibling
//!     substrate (test 2) — the ball's suspected "found a second substrate on
//!     create" does NOT happen; the write path is fenced.
//!   - `bl prime` from a subdirectory DOES bootstrap: it founds a second,
//!     fully-distinct clone bundle — a genuine invisible sibling store for the
//!     same project (test 3). This is the real silent-split entry point.
//!   - From inside a claimed `work/<id>` worktree (a mirrored deep path), the op
//!     addresses yet another bundle and never sees the project's store (test 4).
//!
//! tarpaulin counts `src/` only, so this integration file is coverage-neutral.

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted at `cwd`, HOME/`XDG_STATE_HOME` pinned under the tempdir so the
/// clone bundle never touches the real `$HOME`; the shipped plugins resolve
/// beside the built `bl`. The inherited `BALLS_*` recursion bookkeeping is
/// scrubbed — this file itself runs inside a `bl close` gate under the
/// orchestrator, and a top-level `bl` here must start at depth 0.
fn bl(cwd: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(cwd)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// `git -C <cwd> <args>`, asserting success (plain-git harness setup).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// The XDG layout for the pinned state root — the same pure path arithmetic the
/// binary uses, so a test can name the exact bundle each invocation addresses.
fn xdg(state: &Path) -> balls::layout::Xdg {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
}

/// The `tasks/` directory of the store bundle an invocation at `cwd` addresses.
fn store_tasks(state: &Path, cwd: &Path) -> PathBuf {
    xdg(state).clone_dir(cwd).store().join("tasks")
}

/// How many ball files (anything with an extension) sit in a store's `tasks/`.
fn ball_count(tasks: &Path) -> usize {
    fs::read_dir(tasks)
        .map_or(0, |d| d.filter_map(Result::ok).filter(|e| e.path().extension().is_some()).count())
}

/// How many per-invocation clone bundles exist on this host — one directory per
/// distinct invocation path under `clones/`.
fn bundle_count(state: &Path) -> usize {
    fs::read_dir(xdg(state).clones_dir()).map_or(0, Iterator::count)
}

/// A plain (non-git) project directory with a `src/` subdirectory, primed. The
/// deliverable verbs never require the checkout to be a git repo (they seal onto
/// the STORE, not the project), so a bare directory isolates the cwd-keying from
/// any git machinery. Returns `(home, state, project, subdir)`.
fn primed_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("proj"));
    let sub = project.join("src");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir_all(&home).unwrap();
    bl(&project, &home, &state).arg("prime").assert().success();
    (home, state, project, sub)
}

#[test]
fn a_read_from_a_subdirectory_sees_an_empty_store_not_the_projects_tasks() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project, sub) = primed_project(tmp.path());
    bl(&project, &home, &state).args(["create", "Root task", "--as", "me"]).assert().success();

    // At the project root the ball renders — the store is reachable from here.
    bl(&project, &home, &state).arg("list").assert().success().stdout(contains("Root task"));

    // FINDING (silent split): `invocation_path` is the literal cwd and
    // `clone_dir` percent-encodes it as the WHOLE bundle key, with no git-root
    // discovery in src/. From `project/src` bl therefore addresses a DIFFERENT
    // clones/<enc>/ bundle that has no store, and `list` is an EMPTY success —
    // indistinguishable from "this project has no tasks". Nothing warns.
    bl(&sub, &home, &state).arg("list").assert().success().stdout("");

    // Proof it is a different ADDRESS, not a filtered view: the two stores differ,
    // only the root's holds the ball, and the subdir's was never even founded.
    assert_ne!(store_tasks(&state, &project), store_tasks(&state, &sub));
    assert_eq!(ball_count(&store_tasks(&state, &project)), 1);
    assert!(!store_tasks(&state, &sub).exists(), "the subdir addresses a store that was never founded");
}

#[test]
fn a_mutation_from_a_subdirectory_is_refused_not_a_silent_second_substrate() {
    let tmp = TempDir::new().unwrap();
    let (home, state, _project, sub) = primed_project(tmp.path());

    // FINDING (write path is fenced): a mutating verb is GUARDED — `mutate::primed`
    // refuses when the subdir's clone has no landing `config/` dir, rather than
    // bootstrapping one. So `create` from `project/src` does NOT silently found a
    // sibling substrate as the ball suspected; it errors. The cwd-keying's blast
    // radius is limited for writes to this loud refusal.
    bl(&sub, &home, &state)
        .args(["create", "Sub task", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("no balls checkout here").and(contains("bl prime")));

    // And no second bundle was founded — only the root's clone exists.
    assert_eq!(bundle_count(&state), 1, "a refused mutate founds nothing");
}

#[test]
fn priming_from_a_subdirectory_founds_an_invisible_sibling_substrate() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project, sub) = primed_project(tmp.path());
    bl(&project, &home, &state).args(["create", "Root task", "--as", "me"]).assert().success();

    // FINDING (the real silent-split entry point): unlike a mutate, `bl prime`
    // DOES bootstrap on miss (`checkout::prime` founds the landing for its clone
    // key). Run from `project/src` it founds a SECOND, fully-distinct
    // clones/<enc>/ bundle — an invisible sibling store for the SAME project.
    bl(&sub, &home, &state).arg("prime").assert().success();

    assert_eq!(bundle_count(&state), 2, "prime founded a second substrate keyed on the subdir");
    assert_eq!(ball_count(&store_tasks(&state, &project)), 1, "the root store keeps its ball, untouched");
    assert_eq!(ball_count(&store_tasks(&state, &sub)), 0, "the sibling is a separate, empty store");

    // A read from the subdir now succeeds against the sibling — still empty, so
    // the project's real task stays invisible from here. Two stores, one project.
    bl(&sub, &home, &state).arg("list").assert().success().stdout("");
}

/// A BARE project repo on `main` (balls' common deployment) plus a primed
/// checkout: seed a normal repo, `clone --bare` it, set the identity the
/// delivery `commit-tree` reads, and `bl prime` the stealth store under the
/// tempdir. A git repo is required here because `claim` materializes a code
/// worktree hung off the project root.
fn bare_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state) = (tmp.join("h"), tmp.join("s"));
    fs::create_dir_all(&home).unwrap();
    let seed = tmp.join("seed");
    git(tmp, &["init", "-q", "-b", "main", &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "t"]);
    git(&seed, &["config", "user.email", "t@t"]);
    fs::write(seed.join("seed.txt"), "seed\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "seed"]);
    let project = tmp.join("proj.git");
    git(tmp, &["clone", "-q", "--bare", &seed.to_string_lossy(), &project.to_string_lossy()]);
    git(&project, &["config", "user.name", "t"]);
    git(&project, &["config", "user.email", "t@t"]);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

#[test]
fn a_read_from_inside_a_claimed_work_worktree_never_sees_the_projects_store() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = bare_project(tmp.path());
    let id = stdout(bl(&project, &home, &state).args(["create", "Work task", "--as", "me"]).assert().success());
    let wt = PathBuf::from(stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success()));
    assert!(wt.join("seed.txt").exists(), "claim materialized the code worktree");

    // FINDING: the claimed worktree lives at a MIRRORED path deep under
    // $XDG_STATE_HOME/balls/plugins/bl-delivery/<full-project-path>/<id> — a
    // distinct filesystem location entirely. cwd-keying therefore addresses YET
    // ANOTHER clone bundle, distinct from the project's:
    assert_ne!(store_tasks(&state, &project), store_tasks(&state, &wt));

    // A read from inside the worktree never surfaces the project's task. Two
    // observed sub-behaviors, BOTH consistent with the split: at short paths the
    // encoded key stays under NAME_MAX and `list` is an empty success; at
    // real-world path depths the percent-encoded SINGLE path component exceeds
    // the filesystem's 255-byte limit and the op HARD-FAILS "File name too long
    // (os error 36)" — bl is then wholly unusable from the worktree. Either way
    // the store is unreachable, so pin the invariant that holds regardless: the
    // project's ball is never rendered here.
    bl(&wt, &home, &state).arg("list").assert().stdout(contains("Work task").not());
}
