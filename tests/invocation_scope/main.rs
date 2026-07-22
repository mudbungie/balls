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
//!     same project (test 3). Founding still proceeds (this stays fully
//!     supported — a nested/sibling store is a sane use case), but it is no
//!     longer SILENT: bl-b915 has prime stat the ancestor directories' own
//!     clone dirs first (balls' own record, git never consulted) and warn on
//!     stderr naming the nearest founded ancestor before founding anyway — the
//!     invisible-sibling footgun demoted to a warned deliberate act (test 3b).
//!   - From inside a claimed `work/<id>` worktree (a mirrored deep path), the op
//!     addresses yet another bundle and never sees the project's store (test 4).
//!
//! The global `-C PATH` (bl-c620) is the ESCAPE HATCH for all of it: it replaces
//! `invocation_path` verbatim, so an op addresses the store keyed by PATH from
//! any cwd at all. It is a capability, not a policy — it resolves nothing on its
//! own (no walking, no git-root discovery), which is exactly why any future
//! auto-resolution can layer on top without contradicting it; [`directory`] pins it.
//!
//! Split for the 300-line cap: the cwd-keying findings in [`keying`], the `-C`
//! override in [`directory`]; both share the fixtures below. tarpaulin counts
//! `src/` only, so these integration files are coverage-neutral.

mod directory;
mod keying;

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};

/// `bl` rooted at `cwd`, HOME/`XDG_STATE_HOME` pinned under the tempdir so the
/// clone bundle never touches the real `$HOME`; the shipped plugins resolve
/// beside the built `bl`. The inherited `BALLS_*` recursion bookkeeping is
/// scrubbed — this file itself runs inside a `bl close` gate under the
/// orchestrator, and a top-level `bl` here must start at depth 0.
pub(crate) fn bl(cwd: &Path, home: &Path, state: &Path) -> Command {
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
pub(crate) fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
pub(crate) fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// The XDG layout for the pinned state root — the same pure path arithmetic the
/// binary uses, so a test can name the exact bundle each invocation addresses.
pub(crate) fn xdg(state: &Path) -> balls::layout::Xdg {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy()))
}

/// The `tasks/` directory of the store bundle an invocation at `cwd` addresses.
pub(crate) fn store_tasks(state: &Path, cwd: &Path) -> PathBuf {
    xdg(state).clone_dir(cwd).store().join("tasks")
}

/// How many ball files (anything with an extension) sit in a store's `tasks/`.
pub(crate) fn ball_count(tasks: &Path) -> usize {
    fs::read_dir(tasks)
        .map_or(0, |d| d.filter_map(Result::ok).filter(|e| e.path().extension().is_some()).count())
}

/// How many per-invocation clone bundles exist on this host — one directory per
/// distinct invocation path under `clones/`.
pub(crate) fn bundle_count(state: &Path) -> usize {
    fs::read_dir(xdg(state).clones_dir()).map_or(0, Iterator::count)
}

/// A plain (non-git) project directory with a `src/` subdirectory, primed. The
/// deliverable verbs never require the checkout to be a git repo (they seal onto
/// the STORE, not the project), so a bare directory isolates the cwd-keying from
/// any git machinery. Returns `(home, state, project, subdir)`.
pub(crate) fn primed_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("proj"));
    let sub = project.join("src");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir_all(&home).unwrap();
    bl(&project, &home, &state).arg("prime").assert().success();
    (home, state, project, sub)
}

/// A BARE project repo on `main` (balls' common deployment) plus a primed
/// checkout: seed a normal repo, `clone --bare` it, set the identity the
/// delivery `commit-tree` reads, and `bl prime` the stealth store under the
/// tempdir. A git repo is required here because `claim` materializes a code
/// worktree hung off the project root.
pub(crate) fn bare_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
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
