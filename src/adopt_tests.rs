//! Tests for §6/§13 config adoption (`prime --install CENTER`). A throwaway
//! "center" git repo carries a `balls/config` branch with a distinct config; the
//! landing is the founded substrate. Plugin-free by default (`exe_dir: None`);
//! the two binding tests stand up a fake `protocol`-answering binary beside `bl`
//! (the [`crate::install_tests`] pattern) to reach the validate-and-bind path.

use super::*;
use crate::edge::Edge;
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
        color: false,
        log_level: None,
    }
}

/// Found the two-branch substrate so `adopt` has a landing to copy into; returns
/// the landing checkout.
fn found(e: &Edge) -> PathBuf {
    let clone = e.xdg.clone_dir(&e.invocation_path);
    crate::substrate::found(&clone.landing(), &clone.store(), &e.xdg, e.exe_dir.as_deref()).unwrap();
    clone.landing()
}

/// Build a center repo with a `balls/config` branch carrying the given config;
/// returns the path string `adopt` fetches from.
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

fn head(landing: &Path) -> String {
    git::run(landing, &["rev-parse", "HEAD"], None).unwrap()
}

/// Write an executable `protocol`-answering plugin beside `bl`.
fn plugin(dir: &Path, name: &str, proto: &str, ops: &str) {
    let path = dir.join(name);
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":{proto},\"ops\":{ops}}}'; fi\n"
    );
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

const NO_HOOKS: &str = "[hooks]\n";

#[test]
fn adopt_mirrors_the_centers_config_into_the_landing_and_commits() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let landing = found(&e);
    let before = head(&landing);
    let c = center(tmp.path(), "balls/shared", NO_HOOKS);
    adopt(&e, &landing, &c).unwrap();
    // The landing's config is now the center's — a DISTINCT tasks_branch.
    let cfg = fs::read_to_string(landing.join("config/balls.toml")).unwrap();
    assert!(cfg.contains("balls/shared"), "adopted config: {cfg}");
    assert_ne!(before, head(&landing), "a commit landed");
}

#[test]
fn re_adopting_identical_config_skips_the_commit() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let landing = found(&e);
    let c = center(tmp.path(), "balls/shared", NO_HOOKS);
    adopt(&e, &landing, &c).unwrap();
    let after_first = head(&landing);
    adopt(&e, &landing, &c).unwrap(); // resume / no-op: identical bytes
    assert_eq!(after_first, head(&landing), "idempotent re-adopt skips the commit");
}

#[test]
fn adopt_binds_a_referenced_plugin_whose_binary_is_present() {
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    plugin(&bin, "tracker", "[1]", "[\"sync\",\"prime\"]");
    let e = edge(&tmp, Some(bin.clone()));
    let landing = found(&e);
    let c = center(tmp.path(), "balls/shared", "[hooks]\n\"sync.pre\" = [\"tracker\"]\n");
    adopt(&e, &landing, &c).unwrap();
    let link = landing.join("config/plugins/bin/tracker");
    assert_eq!(fs::read_link(&link).unwrap(), bin.join("tracker"));
}

#[test]
fn adopt_refuses_a_referenced_plugin_the_binary_cannot_serve() {
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    plugin(&bin, "tracker", "[1]", "[\"sync\"]"); // declares sync only
    let e = edge(&tmp, Some(bin));
    let landing = found(&e);
    // The center wires tracker on close.post — an op the binary does not declare.
    let c = center(tmp.path(), "balls/shared", "[hooks]\n\"close.post\" = [\"tracker\"]\n");
    let err = adopt(&e, &landing, &c).unwrap_err();
    assert!(err.to_string().contains("close"), "{err}");
}

#[test]
fn prime_install_adopts_then_drives_prime_and_sync() {
    // The §13 fuse end to end: `prime --install` founds the substrate, adopts the
    // center's config, then this same call's prime+sync chains bring it current.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let c = center(tmp.path(), "balls/shared", NO_HOOKS);
    crate::checkout::prime(&e, &[String::from("--install"), c]).unwrap();
    let landing = e.xdg.clone_dir(&e.invocation_path).landing();
    let cfg = fs::read_to_string(landing.join("config/balls.toml")).unwrap();
    assert!(cfg.contains("balls/shared"), "center config adopted: {cfg}");
    // The driven chains logged their op records (the chain is plugin-free).
    let log = fs::read_to_string(e.xdg.clone_dir(&e.invocation_path).op_log()).unwrap();
    assert!(log.contains("\"op\":\"prime\""), "prime chain ran: {log}");
    assert!(log.contains("\"op\":\"sync\""), "prime drove a sync: {log}");
}
