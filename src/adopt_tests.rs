//! Tests for §6/§13 config adoption. Adoption splits in two: the tracker fetches
//! the center (remote), then core copies it in (local). So the LOCAL half
//! ([`install_local`]) is unit-tested by SIMULATING the fetch — a plain
//! `git fetch` into the landing, playing the tracker's role — and the chain glue
//! ([`adopt`] → `fetch_config`) is tested with fake `install.pre` plugins beside
//! `bl` (the [`crate::install_tests`] pattern). The real end-to-end (the shipped
//! tracker doing the fetch) is `tests/dispatch.rs`.

use super::*;
use crate::edge::Edge;
use crate::git;
use crate::layout::Xdg;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;

/// An edge rooted in `tmp` with the given (optional) `bl`-sibling dir.
fn edge(tmp: &TempDir, exe_dir: Option<PathBuf>) -> Edge {
    Edge {
        xdg: Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        exe_dir,
        path_dirs: Vec::new(),
        color: false,
        log_level: None,
    }
}

/// Found the two-branch substrate; returns the (landing, store) checkouts.
fn found(e: &Edge) -> (PathBuf, PathBuf) {
    let clone = e.xdg.clone_dir(&e.invocation_path);
    crate::substrate::found(&clone.landing(), &clone.store(), &e.xdg, e.exe_dir.as_deref()).unwrap();
    (clone.landing(), clone.store())
}

/// Build a center repo with a `balls/config` branch carrying the given config;
/// returns the path string the fetch pulls from.
fn center(dir: &Path, tasks_branch: &str, plugins: &str) -> String {
    let repo = dir.join("center");
    fs::create_dir_all(&repo).unwrap();
    g(&repo, &["init", "-q", "-b", LANDING_BRANCH]);
    g(&repo, &["config", "user.name", "c"]);
    g(&repo, &["config", "user.email", "c@c"]);
    let config = repo.join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("balls.toml"), format!("tasks_branch = \"{tasks_branch}\"\n")).unwrap();
    fs::write(config.join("plugins.toml"), plugins).unwrap();
    g(&repo, &["add", "-A"]);
    g(&repo, &["commit", "-q", "-m", "center config"]);
    repo.to_string_lossy().into_owned()
}

fn g(cwd: &Path, args: &[&str]) {
    git::run(cwd, args, None).unwrap();
}

/// Play the tracker's role: fetch the center's config branch into the landing,
/// leaving it at `FETCH_HEAD` exactly as the `install.pre` hook would.
fn sim_fetch(landing: &Path, center: &str) {
    g(landing, &["fetch", center, LANDING_BRANCH]);
}

fn head(landing: &Path) -> String {
    git::run(landing, &["rev-parse", "HEAD"], None).unwrap()
}

/// A fake `install`-handling plugin beside `bl`: answers `protocol` (declaring
/// `ops`) and exits 0 for any op/phase WITHOUT fetching — enough to exercise the
/// chain glue, not the real fetch (the dispatch test does that).
fn plugin(dir: &Path, name: &str, ops: &str) {
    let path = dir.join(name);
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":{ops}}}'; fi\n"
    );
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

const NO_HOOKS: &str = "[hooks]\n";

#[test]
fn install_local_mirrors_the_fetched_config_into_the_landing_and_commits() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    let before = head(&landing);
    sim_fetch(&landing, &center(tmp.path(), "balls/shared", NO_HOOKS));
    install_local(&e, &landing).unwrap();
    // The landing's config is now the center's — a DISTINCT tasks_branch.
    let cfg = fs::read_to_string(landing.join("config/balls.toml")).unwrap();
    assert!(cfg.contains("balls/shared"), "adopted config: {cfg}");
    assert_ne!(before, head(&landing), "a commit landed");
}

#[test]
fn re_adopting_identical_config_skips_the_commit() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    let c = center(tmp.path(), "balls/shared", NO_HOOKS);
    sim_fetch(&landing, &c);
    install_local(&e, &landing).unwrap();
    let after_first = head(&landing);
    sim_fetch(&landing, &c);
    install_local(&e, &landing).unwrap(); // resume / no-op: identical bytes
    assert_eq!(after_first, head(&landing), "idempotent re-adopt skips the commit");
}

#[test]
fn install_local_binds_a_referenced_plugin_whose_binary_is_present() {
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    plugin(&bin, "tracker", "[\"install\"]");
    let e = edge(&tmp, Some(bin.clone()));
    let (landing, _store) = found(&e);
    // The adopted config wires tracker on install.pre — an op the binary declares.
    sim_fetch(&landing, &center(tmp.path(), "balls/shared", "[hooks]\n\"install.pre\" = [\"tracker\"]\n"));
    install_local(&e, &landing).unwrap();
    let link = landing.join("config/plugins/bin/tracker");
    assert_eq!(fs::read_link(&link).unwrap(), bin.join("tracker"));
}

#[test]
fn install_local_refuses_a_referenced_plugin_the_binary_cannot_serve() {
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    plugin(&bin, "tracker", "[\"install\"]"); // declares install only
    let e = edge(&tmp, Some(bin));
    let (landing, _store) = found(&e);
    // The adopted config wires tracker on sync.pre — an op the binary lacks.
    sim_fetch(&landing, &center(tmp.path(), "balls/shared", "[hooks]\n\"sync.pre\" = [\"tracker\"]\n"));
    let err = install_local(&e, &landing).unwrap_err();
    assert!(err.to_string().contains("sync"), "{err}");
}

#[test]
fn adopt_without_an_install_pre_plugin_is_an_error() {
    // exe_dir None ⇒ the seed prunes the tracker ⇒ install.pre is empty ⇒ there is
    // no remote-talker to fetch the center: prime --install cannot adopt.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, store) = found(&e);
    let c = center(tmp.path(), "balls/shared", NO_HOOKS);
    let err = adopt(&e, &landing, &store, "me", &c).unwrap_err();
    assert!(err.to_string().contains("install.pre"), "{err}");
}

#[test]
fn adopt_runs_the_install_pre_chain_then_the_local_copy() {
    // A fake tracker is wired on install.pre (by the seed) and bound, so the
    // chain RUNS — but this fake exits without fetching, so the local materialize
    // finds no FETCH_HEAD and adopt surfaces that. This exercises the fetch_config
    // chain loop + the adopt glue; the real fetch is covered in tests/dispatch.rs.
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    plugin(&bin, "tracker", "[\"install\"]");
    let e = edge(&tmp, Some(bin));
    let (landing, store) = found(&e);
    let c = center(tmp.path(), "balls/shared", NO_HOOKS);
    let err = adopt(&e, &landing, &store, "me", &c).unwrap_err();
    // The chain ran (no install.pre error); the missing FETCH_HEAD is what failed.
    assert!(!err.to_string().contains("install.pre"), "the chain ran: {err}");
}
