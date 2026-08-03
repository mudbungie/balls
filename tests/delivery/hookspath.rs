//! bl-63d8: the delivery gate (bl-ee85) resolves the project's `pre-commit` hook
//! with `git rev-parse --git-path hooks/pre-commit`, *exactly as git resolves
//! it* — so a project that redirects its hooks with `core.hooksPath` is gated by
//! THAT hook, and a stale `.git/hooks/pre-commit` decoy is ignored, just as it
//! would be for a porcelain commit. These drive the real `bl-delivery` binary's
//! `close.pre` and prove the custom hook gates in BOTH directions (a failing one
//! aborts the close, a passing one delivers) while the decoy never runs.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use balls::delivery_path::worktree_path;
use balls::layout::Xdg;
use predicates::str::contains;
use tempfile::TempDir;

use crate::{change_dir, delivery, post, pre, project};

/// Write `script` as an executable file at `path`, creating parent dirs.
fn install_exec(path: &Path, script: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// `git -C root log -1 --format=%s main` — the integration tip's subject.
fn main_subject(root: &Path) -> String {
    let o = Command::new("git").current_dir(root).args(["log", "-1", "--format=%s", "main"]).output().unwrap();
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// Claim `bl-x`, point the project's hooks at an absolute custom dir, and plant
/// a LOUD decoy at the default `.git/hooks/pre-commit` that both drops a sentinel
/// file and exits non-zero — so a run of the decoy is observable AND would flip
/// the gate's verdict. Returns `(tmp, home, root, worktree, custom_dir, sentinel)`.
fn claimed_with_redirect() -> (TempDir, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap().to_string();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", &inv, "bl-x");
    delivery(&root, &home, "claim", "post", &post(&inv, "bl-x", "Add feature")).assert().success();

    // Redirect hooks to an absolute custom dir (relative would resolve against
    // the worktree's top level — absolute is unambiguous across linked trees).
    let custom = tmp.path().join("myhooks");
    let cfg = Command::new("git")
        .current_dir(&root)
        .args(["config", "core.hooksPath", custom.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(cfg.success(), "git config core.hooksPath failed");
    let sentinel = tmp.path().join("decoy-ran");
    // Decoy at the DEFAULT hooks path: if git (and thus the gate) ever consulted
    // it, this touches the sentinel and aborts — neither must happen.
    install_exec(
        &root.join(".git/hooks/pre-commit"),
        &format!("#!/bin/sh\ntouch {}\nexit 1\n", sentinel.display()),
    );
    (tmp, home, root, wt, custom, sentinel)
}

#[test]
fn a_failing_custom_hookspath_hook_aborts_the_close_and_the_decoy_never_runs() {
    // core.hooksPath points at the custom dir; its pre-commit FAILS. The gate
    // must honor it — the close aborts, main never moves, the task stays
    // claimed (worktree up). The default-path decoy is never consulted.
    let (tmp, home, root, wt, custom, sentinel) = claimed_with_redirect();
    let inv = root.to_str().unwrap();
    fs::write(wt.join("feature.txt"), "broken\n").unwrap();
    install_exec(&custom.join("pre-commit"), "#!/bin/sh\nexit 1\n");

    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("delivery gate"));

    assert_eq!(main_subject(&root), "seed", "the custom hook must abort the delivery");
    assert!(wt.join("feature.txt").exists(), "the worktree stays up for the fix");
    assert!(!sentinel.exists(), "the .git/hooks decoy must NOT run under core.hooksPath");
}

#[test]
fn a_passing_custom_hookspath_hook_delivers_ignoring_the_failing_decoy() {
    // The mirror: core.hooksPath's pre-commit SUCCEEDS (and proves it ran in the
    // WORKTREE by requiring the work's own file in $PWD). Delivery lands. The
    // failing default-path decoy is ignored — had it run, the close would have
    // aborted and the sentinel would exist.
    let (tmp, home, root, wt, custom, sentinel) = claimed_with_redirect();
    let inv = root.to_str().unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    install_exec(&custom.join("pre-commit"), "#!/bin/sh\ntest -f feature.txt\n");

    let change = change_dir(tmp.path(), "change");
    let mut close = delivery(&change, &home, "close", "pre", &pre(inv, "Add feature"));
    for var in [
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_AUTHOR_DATE",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        "GIT_COMMITTER_DATE",
    ] {
        close.env_remove(var);
    }
    close.assert().success();

    assert_eq!(main_subject(&root), "Add feature [bl-x]", "the passing custom hook must let delivery land");
    assert!(!sentinel.exists(), "the failing .git/hooks decoy must NOT run under core.hooksPath");
    let identity = Command::new("git")
        .current_dir(&root)
        .args(["log", "-1", "--format=%an%n%ae%n%cn%n%ce", "main"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(identity.stdout).unwrap(),
        "test\ntest@example.com\ntest\ntest@example.com\n",
        "delivery must retain repository-local author and committer config"
    );
}

#[test]
fn hostile_indexed_config_cannot_suppress_the_repository_gate() {
    // bl-1ec6's incident reproducer: command-scope config outranks local
    // `core.hooksPath`, so an inherited `/dev/null` used to make the configured
    // hook disappear. The real binary must strip the whole indexed family both
    // from its Git children and from the selected hook (whose nested Git must
    // also see the repository-local value).
    let (tmp, home, root, wt, custom, decoy) = claimed_with_redirect();
    let inv = root.to_str().unwrap();
    fs::write(wt.join("feature.txt"), "must not land\n").unwrap();
    let ran = tmp.path().join("configured-hook-ran");
    install_exec(
        &custom.join("pre-commit"),
        &format!(
            "#!/bin/sh\n\
             test \"$(git config --get core.hooksPath)\" = \"{}\" || exit 90\n\
             if env | grep -E '^(GIT_CONFIG_COUNT|GIT_CONFIG_KEY_0|GIT_CONFIG_VALUE_0)=' >/dev/null; then exit 91; fi\n\
             touch \"{}\"\n\
             exit 1\n",
            custom.display(),
            ran.display()
        ),
    );

    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/dev/null")
        .assert()
        .failure()
        .code(1)
        .stderr(contains("delivery gate"));

    assert!(ran.exists(), "the repository-configured hook did not run");
    assert_eq!(
        main_subject(&root),
        "seed",
        "a suppressed gate would have moved main"
    );
    assert!(
        !decoy.exists(),
        "the default hook decoy ran instead of the configured hook"
    );
}
