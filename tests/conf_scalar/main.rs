//! End-to-end harness for the scalar `bl conf` surface (bl-d2d8) — the local,
//! chainless config CRUD (§4/§12). Every test drives the freshly-built `bl`
//! against a throwaway git repo on `main` in a temp dir, with `HOME` +
//! `$XDG_STATE_HOME` pinned into the tempdir and `XDG_CONFIG_HOME` removed, so
//! nothing touches the real `~/.local/state` store. `conf` seals nothing to the
//! store and dispatches no plugin, so the oracle is purely the printed value,
//! the stderr provenance line, and the on-disk config files.
//!
//! Split for the 300-line cap: reads (dump + provenance ladder) in [`reads`],
//! writes + refusals in [`writes`]; both share the helpers below.

mod reads;
mod writes;

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;

/// `bl` rooted in `project`, `HOME`/`$XDG_STATE_HOME` pinned into the tempdir and
/// `XDG_CONFIG_HOME` removed, plus the inherited plugin-chain env scrubbed so a
/// `cargo test` launched from inside a close-hook never leaks depth/name.
pub(crate) fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success.
pub(crate) fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A bare project repo on `main` (git only, no `bl prime`) — the pre-checkout
/// state for the "conf before prime" case.
pub(crate) fn bare_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    (project, home, state)
}

/// A real project repo on `main` with a primed balls checkout (no `origin`, so a
/// fresh checkout reads circumstantial stealth).
pub(crate) fn primed_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (project, home, state) = bare_project(tmp);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// The landing worktree path bl resolved for this checkout, parsed from the
/// dump's `landing  <path>` line — the anchor for the on-disk config files.
pub(crate) fn landing(project: &Path, home: &Path, state: &Path) -> PathBuf {
    let out = bl(project, home, state).arg("conf").output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let line = text.lines().find(|l| l.starts_with("landing ")).expect("dump prints a landing path line");
    PathBuf::from(line.split_whitespace().nth(1).unwrap())
}

/// The three checkout config files behind a landing (§4): the committed
/// `balls.toml`/`plugins.toml` under `config/config/`, and the local-state
/// `binding.toml` beside the landing checkout. Absent ⇒ empty string.
pub(crate) fn balls_toml(land: &Path) -> String {
    read_opt(&land.join("config").join("balls.toml"))
}
pub(crate) fn plugins_toml(land: &Path) -> String {
    read_opt(&land.join("config").join("plugins.toml"))
}
pub(crate) fn binding_toml(land: &Path) -> String {
    read_opt(&land.parent().unwrap().join("binding.toml"))
}
fn read_opt(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Assert `bl conf <key>` prints exactly `value` on stdout and the constructed
/// `conf: <key> from <layer>` provenance on stderr.
pub(crate) fn read_is(project: &Path, home: &Path, state: &Path, key: &str, value: &str, layer: &str) {
    bl(project, home, state)
        .args(["conf", key])
        .assert()
        .success()
        .stdout(format!("{value}\n"))
        .stderr(contains(format!("conf: {key} from {layer}")));
}
