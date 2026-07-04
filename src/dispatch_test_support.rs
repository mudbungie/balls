//! Shared fixtures for the [`crate::dispatch`] tests ([`super::tests`] and
//! [`super::help_tests`]) — a plugin-free [`Edge`], the `run` shim, and the
//! landing/store/op-log path readers both suites reach for.

use crate::edge::Edge;
use std::path::Path;
use tempfile::TempDir;

/// An edge rooted in `tmp` with no plugin binaries installed (stealth) — prime
/// founds substrate, the seed prunes every default hook, and the chain runs
/// empty, so `run` needs no plugin subprocess.
pub(crate) fn edge(tmp: &TempDir) -> Edge {
    Edge {
        xdg: crate::layout::Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        exe_dir: None,
        path_dirs: Vec::new(),
        color: false,
        log_level: None,
    }
}

pub(crate) fn run_in(tmp: &TempDir, args: &[&str]) -> i32 {
    crate::run(&edge(tmp), &args.iter().map(ToString::to_string).collect::<Vec<_>>())
}

/// Init a git repo at `tmp/proj` (the edge's invocation path) with one
/// `seed`-content commit; returns its root-commit hash. Distinct `seed`s ⇒
/// distinct roots, so a re-root test never trips the same-second SHA flake.
pub(crate) fn git_root(tmp: &TempDir, seed: &str) -> String {
    use crate::delivery_repo::Project;
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    let g = |args: &[&str]| Project::run(&proj, args).unwrap();
    g(&["init", "-q", "-b", "main"]);
    g(&["config", "user.name", "t"]);
    g(&["config", "user.email", "t@e.com"]);
    std::fs::write(proj.join("f.txt"), seed).unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-q", "-m", "seed"]);
    Project::at(&proj).root_commit().unwrap()
}

/// The landing checkout for `tmp`'s edge.
pub(crate) fn landing(tmp: &TempDir) -> std::path::PathBuf {
    edge(tmp).xdg.clone_dir(Path::new(&edge(tmp).invocation_path)).landing()
}

/// The store checkout for `tmp`'s edge.
pub(crate) fn store(tmp: &TempDir) -> std::path::PathBuf {
    edge(tmp).xdg.clone_dir(Path::new(&edge(tmp).invocation_path)).store()
}

/// The unified op log path for `tmp`'s edge.
pub(crate) fn op_log(tmp: &TempDir) -> std::path::PathBuf {
    edge(tmp).xdg.clone_dir(Path::new(&edge(tmp).invocation_path)).op_log()
}

/// The single ball id under `tasks/` (basename minus `.md`).
pub(crate) fn sole_task_id(tasks: &Path) -> String {
    let mut ids: Vec<String> = std::fs::read_dir(tasks)
        .unwrap()
        .filter_map(|e| e.unwrap().file_name().to_string_lossy().strip_suffix(".md").map(str::to_string))
        .collect();
    assert_eq!(ids.len(), 1, "expected exactly one ball");
    ids.pop().unwrap()
}
