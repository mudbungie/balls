//! Tests for the `bl install` run-wiring: the `--to` checkout resolution and
//! the engine-sealed path-copy driven end to end on a founded substrate (the
//! [`crate::adopt_tests`] pattern — throwaway repos, fake plugins beside `bl`
//! where a chain participant is needed). The fixtures are `pub(crate)` so the
//! sibling §6 surface tests ([`super::surface_tests`]) share them; the argv
//! parse tests live with the parser ([`super::args`]).

use super::*;
use crate::git;
use crate::layout::Xdg;
use crate::substrate;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// An edge rooted in `tmp` with the given (optional) `bl`-sibling dir.
pub(crate) fn edge(tmp: &TempDir, exe_dir: Option<PathBuf>) -> Edge {
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
pub(crate) fn found(e: &Edge) -> (PathBuf, PathBuf) {
    let clone = e.xdg.clone_dir(&e.invocation_path);
    substrate::found(&clone.landing(), &clone.store(), &e.xdg, e.exe_dir.as_deref()).unwrap();
    (clone.landing(), clone.store())
}

pub(crate) fn g(cwd: &Path, args: &[&str]) {
    git::run(cwd, args, None).unwrap();
}

pub(crate) fn head(checkout: &Path) -> String {
    git::run(checkout, &["rev-parse", "HEAD"], None).unwrap()
}

pub(crate) fn run_install(e: &Edge, args: &[&str]) -> io::Result<()> {
    run(e, &args.iter().map(ToString::to_string).collect::<Vec<_>>())
}

/// A `side` branch of the landing whose `config/balls.toml` differs — a local
/// ref a standalone install can pull from.
fn side_branch(tmp: &TempDir, landing: &Path) -> &'static str {
    let wt = tmp.path().join("side-wt");
    g(landing, &["branch", "side"]);
    g(landing, &["worktree", "add", "-q", &wt.to_string_lossy(), "side"]);
    fs::write(wt.join("config/balls.toml"), "tasks_branch = \"balls/elsewhere\"\n").unwrap();
    g(&wt, &["commit", "-q", "-am", "side config"]);
    "side"
}

#[test]
fn install_before_prime_is_an_error() {
    let tmp = TempDir::new().unwrap();
    let err = run_install(&edge(&tmp, None), &["--from", "balls/tasks"]).unwrap_err();
    assert!(err.to_string().contains("bl prime"), "{err}");
}

#[test]
fn an_unresolvable_to_ref_is_refused() {
    // Sealing targets only the two local checkouts (§2); a remote/other ref is
    // the open bl-66e7 direction and must be refused, not guessed.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    found(&e);
    let err = run_install(&e, &["--from", "balls/tasks", "--to", "nope"]).unwrap_err();
    assert!(err.to_string().contains("--to must name"), "{err}");
}

#[test]
fn an_option_like_from_ref_is_refused() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    found(&e);
    let err = run_install(&e, &["--from", "--upload-pack=evil"]).unwrap_err();
    assert!(err.to_string().contains("refusing"), "{err}");
}

#[test]
fn a_missing_from_ref_is_a_git_error() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    found(&e);
    let err = run_install(&e, &["--from", "no-such-ref"]).unwrap_err();
    assert!(err.to_string().contains("worktree add"), "{err}");
}

#[test]
fn install_seals_a_path_copy_onto_the_landing_tip() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    let from = side_branch(&tmp, &landing);
    let before = head(&landing);

    run_install(&e, &["config", "--from", from, "--to", LANDING_BRANCH]).unwrap();

    // The landing carries the side branch's config, via a NEW sealed commit.
    let cfg = fs::read_to_string(landing.join("config/balls.toml")).unwrap();
    assert!(cfg.contains("balls/elsewhere"), "installed config: {cfg}");
    assert_ne!(before, head(&landing), "a commit sealed");
    // Both ephemeral worktrees (source + change) are torn down.
    let clone = e.xdg.clone_dir(&e.invocation_path);
    assert!(!clone.change("install-src").exists());
    assert!(!clone.change("install").exists());
}

#[test]
fn an_install_seal_carries_the_checkout_scoped_trailers() {
    // §5: checkout-scoped seals carry bl-protocol/bl-op/bl-actor — only bl-id
    // (which names a single ball) is absent (bl-1d9b).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    let from = side_branch(&tmp, &landing);
    run_install(&e, &["config", "--from", from, "--as", "me"]).unwrap();
    let msg = git::run(&landing, &["log", "-1", "--format=%B"], None).unwrap();
    let md = crate::message::parse(&msg).unwrap();
    assert_eq!(md["bl-protocol"], ["1"], "{msg}");
    assert_eq!(md["bl-op"], ["install"], "{msg}");
    assert_eq!(md["bl-actor"], ["me"], "{msg}");
    assert!(!md.contains_key("bl-id"), "{msg}");
}

#[test]
fn reinstalling_identical_content_converges_on_the_tip() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    let from = side_branch(&tmp, &landing);
    run_install(&e, &["config", "--from", from]).unwrap(); // --to defaults to the landing
    let sealed = head(&landing);
    run_install(&e, &["config", "--from", from]).unwrap();
    assert_eq!(sealed, head(&landing), "the no-op seal lands no empty commit");
}

#[test]
fn install_can_target_the_configured_store_branch() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (_landing, store) = found(&e);
    let before = head(&store);
    // tasks ← tasks: a byte-identical mirror — exercises the store-target
    // resolution and converges without touching the tip.
    run_install(&e, &["tasks", "--from", crate::DEFAULT_TASKS_BRANCH, "--to", crate::DEFAULT_TASKS_BRANCH]).unwrap();
    assert_eq!(before, head(&store));
}

#[test]
fn a_failing_pre_plugin_aborts_the_seal_and_cleans_up() {
    // A fake tracker beside `bl` declares `install` (so the seed wires + binds
    // it on install.pre) but exits 1 on every hook — the engine aborts, §14
    // unwinds, and the landing tip stands.
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let plugin = bin.join("tracker");
    fs::write(
        &plugin,
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{\"protocol\":[1],\"ops\":[\"install\"]}'; exit 0; fi\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&plugin, fs::Permissions::from_mode(0o755)).unwrap();
    let e = edge(&tmp, Some(bin));
    let (landing, _store) = found(&e);
    let before = head(&landing);

    let err = run_install(&e, &["config", "--from", LANDING_BRANCH]).unwrap_err();
    assert!(err.to_string().contains("tracker"), "{err}");
    assert_eq!(before, head(&landing), "the abort unwound — nothing sealed");
    let clone = e.xdg.clone_dir(&e.invocation_path);
    assert!(!clone.change("install-src").exists(), "the source worktree is torn down on abort");
}
