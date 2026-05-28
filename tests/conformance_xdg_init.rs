//! Phase 1B conformance gates for SPEC-clone-layout §14 — the
//! write-side behaviors `bl init` must enforce. Sibling of
//! `conformance_xdg_layout.rs` (Phase 1A read-side gates).
//!
//! Gates covered here:
//!
//! - §14.1 — XDG paths from a fresh `bl init`. No `.balls/` at the
//!   clone root, tracker checkout materialized at
//!   `~/.local/state/balls/trackers/<enc-origin>/<enc-balls-tasks>/`,
//!   `repo.json` + `project.json` on the own branch, no `tracker.json`
//!   in the solo case, no `balls: initialize` commit on `main`.
//! - §14.4 — bootstrap branch is constant. A `repo.json` field
//!   purporting to override the branch name does not change what
//!   `bl` fetches.
//! - §14.15 — stealth init writes `clone.json`. Subsequent `bl`
//!   invocations from that directory read `tasks_dir` from
//!   `clone.json` and never look for a tracker.
//! - §14.19 (half-gated) — across `bl init` + `bl create`, `git log
//!   main` gains no `balls:` commits. The full lifecycle gate (claim,
//!   review, close) lands with Phase 1C; what we can prove here is
//!   that init + create do not write on `main`.
//!
//! Error and idempotency gates for `init_xdg` live in the sibling
//! `conformance_xdg_init_paths` test bundle.

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component, ENC_BALLS_TASKS};
use balls::xdg_paths::{clone_json_path, own_tracker_checkout};
use common::tmp;
use common::xdg_init::{bases, bl_xdg, fresh_clone_into, init_xdg, main_log};
use common::Repo;
use std::fs;

fn new_bare_remote() -> Repo {
    common::new_bare_remote()
}

// -- §14.1 -- XDG paths from a fresh bl init --

#[test]
fn spec_14_1_bl_init_writes_xdg_only() {
    let home = tmp();
    let remote = new_bare_remote();
    let origin_url = remote.path().to_string_lossy().into_owned();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");

    init_xdg(&clone, home.path(), false, None);

    // No `.balls/` at the clone root.
    assert!(
        !clone.join(".balls").exists(),
        "XDG bl init must not create .balls/ at the clone root"
    );
    // No balls-related entries in .gitignore (the file should not
    // exist at all, since bl init does not touch it).
    let gi = clone.join(".gitignore");
    if gi.exists() {
        let s = fs::read_to_string(&gi).unwrap();
        assert!(
            !s.contains(".balls"),
            ".gitignore must not carry balls entries after XDG bl init"
        );
    }

    // No `balls:`-prefixed commits on main.
    let log = main_log(&clone);
    for line in &log {
        assert!(
            !line.starts_with("balls:"),
            "main must not gain a balls: commit from bl init, got: {line}"
        );
    }

    // Tracker checkout materialized at the documented XDG path with
    // repo.json + project.json + no tracker.json (solo case).
    let bases = bases(home.path());
    let enc_origin = percent_encode_component(&canonicalize_origin(&origin_url));
    let own = own_tracker_checkout(&bases, &enc_origin);
    assert!(own.join(".git").exists(), "tracker checkout must exist");
    assert!(own.join(".balls/repo.json").exists(), "repo.json on own branch");
    assert!(
        own.join(".balls/project.json").exists(),
        "project.json on own branch"
    );
    assert!(
        !own.join(".balls/tracker.json").exists(),
        "no tracker.json in solo case"
    );
    assert!(
        own.join(".balls/tasks").exists(),
        "tasks/ scaffolded on tracker branch"
    );
}

// -- §14.4 -- bootstrap branch is constant --

#[test]
fn spec_14_4_bootstrap_branch_is_constant() {
    let home = tmp();
    let remote = new_bare_remote();
    let origin_url = remote.path().to_string_lossy().into_owned();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");

    init_xdg(&clone, home.path(), false, None);

    // bl fetched `balls/tasks` (encoded as balls%2Ftasks) — not any
    // other branch name.
    let bases = bases(home.path());
    let enc_origin = percent_encode_component(&canonicalize_origin(&origin_url));
    let expected = bases
        .state_root()
        .join("trackers")
        .join(&enc_origin)
        .join(ENC_BALLS_TASKS);
    assert!(
        expected.join(".git").exists(),
        "tracker checkout must live at balls%2Ftasks path, not a custom branch name"
    );

    // Even if `repo.json` is hand-edited to claim a different branch,
    // `bl` still fetches `balls/tasks`. The bootstrap branch is a
    // binary constant per SPEC §5.
    let repo_json_path = expected.join(".balls/repo.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&repo_json_path).unwrap()).unwrap();
    v.as_object_mut()
        .unwrap()
        .insert("state_branch".into(), serde_json::json!("custom/branch"));
    fs::write(&repo_json_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    // bl list still finds the tracker at balls%2Ftasks — repo.json
    // cannot override the bootstrap branch.
    bl_xdg(&clone, home.path()).arg("list").assert().success();
    assert!(expected.join(".git").exists());
}

// -- §14.15 -- stealth init writes clone.json --

#[test]
fn spec_14_15_stealth_init_writes_clone_json() {
    let home = tmp();
    let stealth_root = home.path().join("stealth-proj");
    fs::create_dir_all(&stealth_root).unwrap();
    let stealth_root = fs::canonicalize(&stealth_root).unwrap();
    let tasks_dir = home.path().join("tasks-store");
    fs::create_dir_all(&tasks_dir).unwrap();
    let tasks_dir = fs::canonicalize(&tasks_dir).unwrap();

    init_xdg(
        &stealth_root,
        home.path(),
        true,
        Some(tasks_dir.to_string_lossy().into_owned()),
    );

    // SPEC §4.1: clone.json is keyed by the --tasks-dir, not by the
    // cwd, when --tasks-dir is given.
    let bases = bases(home.path());
    let nested = balls::encoding::nested_clone_path(&tasks_dir);
    let cj_path = clone_json_path(&bases, &nested);
    assert!(cj_path.exists(), "clone.json at {}", cj_path.display());

    let cj: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cj_path).unwrap()).unwrap();
    assert_eq!(cj["stealth"], serde_json::json!(true));
    assert_eq!(
        cj["tasks_dir"].as_str().unwrap(),
        tasks_dir.to_string_lossy()
    );

    // No tracker checkout, no .balls/ at the clone root.
    assert!(!stealth_root.join(".balls").exists());
    assert!(
        !bases.state_root().join("trackers").exists(),
        "stealth init must not materialize trackers/"
    );

    // SPEC §4.1: identity is the --tasks-dir when given. Subsequent
    // bl ops run from the tasks_dir (clone.json is keyed by it).
    let out = bl_xdg(&tasks_dir, home.path())
        .args(["create", "stealth task"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bl create in stealth: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task_files: Vec<_> = fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("bl-")
        })
        .collect();
    assert!(
        !task_files.is_empty(),
        "task file must land under stealth tasks_dir"
    );
}

// -- §14.19 -- no balls: commits on main across init + create --

#[test]
fn spec_14_19_no_balls_commits_on_main_for_init_create() {
    let home = tmp();
    let remote = new_bare_remote();
    let origin_url = remote.path().to_string_lossy().into_owned();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");

    let pre_log = main_log(&clone);
    init_xdg(&clone, home.path(), false, None);
    bl_xdg(&clone, home.path())
        .args(["create", "first task"])
        .assert()
        .success();
    let post_log = main_log(&clone);

    // No new commits on main from either operation.
    assert_eq!(
        pre_log, post_log,
        "main must be unchanged across bl init + bl create under XDG layout"
    );
}

// -- §14.19 (full lifecycle) -- exactly one [bl-xxxx] delivery on
//                                main; init/create/claim/close add
//                                no commits of their own.

/// Full §14.19: across `bl init → create → claim → review → close`,
/// `git log main` grows by exactly one commit — the squash delivery
/// tagged `[bl-xxxx]`. No `balls:` commits land on main along the way.
#[test]
fn spec_14_19_full_lifecycle_leaves_only_delivery_on_main() {
    let home = tmp();
    let remote = new_bare_remote();
    let origin_url = remote.path().to_string_lossy().into_owned();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");
    // Seed `main` so HEAD resolves before bl init — XDG init writes
    // nothing on main.
    run_git(&clone, &["commit", "--allow-empty", "-qm", "seed", "--no-verify"]);
    run_git(&clone, &["push", "-q", "origin", "main"]);
    let pre_log = main_log(&clone);

    init_xdg(&clone, home.path(), false, None);
    let create_out = bl_xdg(&clone, home.path())
        .args(["create", "feature"])
        .output()
        .expect("bl create");
    assert!(create_out.status.success(), "bl create failed");
    let id = String::from_utf8_lossy(&create_out.stdout).trim().to_string();

    bl_xdg(&clone, home.path()).args(["claim", &id]).assert().success();
    let canon = std::fs::canonicalize(&clone).unwrap();
    let nested = balls::encoding::nested_clone_path(&canon);
    let per = balls::xdg_paths::PerClonePaths::new(&bases(home.path()), &nested);
    fs::write(per.worktree_for(&id).join("feature.txt"), "real work").unwrap();
    bl_xdg(&clone, home.path()).args(["review", &id, "-m", "ship it"]).assert().success();
    bl_xdg(&clone, home.path()).args(["close", &id, "-m", "done"]).assert().success();

    let post_log = main_log(&clone);
    let new_commits: Vec<&String> = post_log
        .iter()
        .take(post_log.len().saturating_sub(pre_log.len()))
        .collect();
    assert_eq!(
        new_commits.len(), 1,
        "main must gain exactly one commit, got: {new_commits:?}"
    );
    let subject = new_commits[0];
    assert!(subject.contains(&format!("[{id}]")), "missing [{id}] tag: {subject}");
    assert!(!subject.starts_with("balls:"), "no `balls:` on main: {subject}");
}

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(cwd).args(args).output()
        .unwrap_or_else(|e| panic!("git {} failed: {e}", args.join(" ")));
    assert!(out.status.success(), "git {} failed: {}",
        args.join(" "), String::from_utf8_lossy(&out.stderr));
}
