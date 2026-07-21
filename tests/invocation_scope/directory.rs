//! The global `-C PATH` override (bl-c620): the explicit escape
//! hatch from the cwd-keying [`crate::keying`] pins. `-C` replaces the
//! invocation path verbatim, so the store addressed is exactly the one keyed by
//! PATH — from any cwd, with no walking and no git-root discovery.

use crate::*;
use predicates::str::contains;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn a_directory_override_addresses_the_project_store_from_an_unrelated_cwd() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project, _sub) = primed_project(tmp.path());
    let elsewhere = tmp.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let dash_c = |args: &[&str]| {
        let mut a = vec!["-C".to_string(), project.to_string_lossy().into_owned()];
        a.extend(args.iter().map(ToString::to_string));
        let mut cmd = bl(&elsewhere, &home, &state);
        cmd.args(a);
        cmd
    };

    // A mutation from a cwd with no substrate of its own still lands in the
    // project's store, and a read from there renders it.
    dash_c(&["create", "Remote-driven task", "--as", "me"]).assert().success();
    dash_c(&["list"]).assert().success().stdout(contains("Remote-driven task"));
    assert_eq!(ball_count(&store_tasks(&state, &project)), 1, "the ball landed in the PROJECT's store");

    // And nothing was founded under the cwd we actually ran in: `-C` addresses,
    // it does not fork.
    assert_eq!(bundle_count(&state), 1, "-C founded no bundle for the unrelated cwd");
    assert!(!store_tasks(&state, &elsewhere).exists());
}

#[test]
fn a_directory_override_behaves_exactly_as_if_bl_had_run_there() {
    let tmp = TempDir::new().unwrap();
    let (home, state, _project, sub) = primed_project(tmp.path());
    let s = sub.to_string_lossy().into_owned();
    let elsewhere = tmp.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let at_sub = |args: &[&str]| {
        let mut cmd = bl(&elsewhere, &home, &state);
        cmd.arg("-C").arg(&s).args(args);
        cmd
    };

    // `-C` a path with NO store reproduces cwd-there behavior exactly (tests 1-3):
    // a read is a silent empty success, a mutation is refused, `prime` founds.
    at_sub(&["list"]).assert().success().stdout("");
    at_sub(&["create", "Sub task", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("no balls checkout here"));
    at_sub(&["prime"]).assert().success();
    assert_eq!(bundle_count(&state), 2, "-C prime founded the subdir's own bundle, same as cwd there");
    assert!(store_tasks(&state, &sub).exists());
}

#[test]
fn a_directory_override_naming_no_directory_is_refused() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project, _sub) = primed_project(tmp.path());
    // Nonexistent: refused in balls voice before any op runs — never a silent
    // fall back to the cwd, which would address the wrong store.
    bl(&project, &home, &state)
        .args(["-C", &tmp.path().join("nope").to_string_lossy(), "list"])
        .assert()
        .failure()
        .stderr(contains("no such directory"));
    // A FILE is not a directory either, and a dangling `-C` is a usage error.
    let file = tmp.path().join("f.txt");
    fs::write(&file, "x").unwrap();
    bl(&project, &home, &state)
        .args(["-C", &file.to_string_lossy(), "list"])
        .assert()
        .failure()
        .stderr(contains("no such directory"));
    bl(&project, &home, &state).arg("-C").assert().failure().stderr(contains("-C needs a value"));
}

#[test]
fn a_claim_through_the_override_hangs_the_worktree_off_the_named_project() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = bare_project(tmp.path());
    let elsewhere = tmp.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let dash_c = |args: &[&str]| {
        let mut a = vec!["-C".to_string(), project.to_string_lossy().into_owned()];
        a.extend(args.iter().map(ToString::to_string));
        let mut cmd = bl(&elsewhere, &home, &state);
        cmd.args(a);
        cmd
    };

    // The override reaches the PLUGIN chain, not just core: `bl-delivery` resolves
    // the code repo and mirrors its worktree territory from the same overridden
    // invocation path, so a claim driven from an unrelated cwd materializes a
    // worktree hung off the named project — the cwd is never consulted.
    let id = stdout(dash_c(&["create", "Claimed elsewhere", "--as", "me"]).assert().success());
    let wt = PathBuf::from(stdout(dash_c(&["claim", &id, "--as", "me"]).assert().success()));
    assert!(wt.join("seed.txt").exists(), "the project's code is checked out in the worktree");
    assert!(wt.starts_with(&state), "the worktree lives in delivery's territory under the pinned state");
    assert!(
        wt.to_string_lossy().contains(&*project.to_string_lossy()),
        "the mirrored path names the -C project ({}), not the cwd: {}",
        project.display(),
        wt.display()
    );
}
