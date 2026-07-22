//! The cwd-keying FINDINGS (tests 1-4): what addressing the substrate by the
//! literal current directory actually does to a read, a mutation, a `prime`,
//! and an op run from inside a claimed `work/<id>` worktree. See [`crate`] for
//! the architecture note and the shared fixtures.

use crate::*;
use predicates::prelude::*;
use predicates::str::contains;
use std::path::PathBuf;
use tempfile::TempDir;

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


#[test]
fn priming_a_subdir_under_a_primed_project_warns_which_ancestor_was_meant() {
    let tmp = TempDir::new().unwrap();
    let (home, state, project, sub) = primed_project(tmp.path());

    // bl-b915: unlike the SILENT sibling-founding in the finding above, prime
    // now scans ancestors (balls' own clone-dir record, no git involved) and
    // warns before founding a second bundle for the subdir — naming the
    // ancestor it suspects was meant, and the `-C` escape hatch (bl-c620) back
    // to it. Still a WARNING only: the founding proceeds regardless.
    bl(&sub, &home, &state)
        .arg("prime")
        .assert()
        .success()
        .stderr(contains("an existing store sits at").and(contains(project.to_string_lossy().as_ref())).and(contains("bl -C")));
    assert_eq!(bundle_count(&state), 2, "the advisory never blocks founding — a second bundle still lands");
}

#[test]
fn priming_at_an_unrelated_directory_carries_no_ancestor_warning() {
    let tmp = TempDir::new().unwrap();
    let (home, state, _project, _sub) = primed_project(tmp.path());
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();

    // No founded store sits above `elsewhere` (a sibling of `project`, not a
    // descendant) — the ancestor advisory stays exactly as silent as before
    // bl-b915 (a non-git-repo notice from the delivery plugin is unrelated and
    // still fires here — only the ancestor-warning text is pinned absent).
    bl(&elsewhere, &home, &state).arg("prime").assert().success().stderr(contains("an existing store sits at").not());
    assert_eq!(bundle_count(&state), 2);
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

    // …and `-C <project-root>` is the way back: from inside the worktree the op
    // addresses the PROJECT's store and the ball renders. This also sidesteps the
    // ENAMETOOLONG failure above — the deep worktree path is never encoded.
    bl(&wt, &home, &state)
        .args(["-C", &project.to_string_lossy(), "list"])
        .assert()
        .success()
        .stdout(contains("Work task"));
}
