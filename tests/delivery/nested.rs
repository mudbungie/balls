//! §11 NESTED DELIVERY end to end (bl-7b71) through the real `bl-delivery`
//! binary: a ball whose §7 `command.target` names its epic forks that epic's
//! ref, delivers back into it, and leaves `main` untouched — then the epic
//! itself, target-less, lands the accumulated work on `main` as ONE commit.
//!
//! "Done" stops meaning "on main": it means delivered to MY target, and main is
//! simply the target of a ball with no parent. Flat delivery is the degenerate
//! case, so the flat lifecycle tests in [`crate`] are the other half of this
//! matrix and are deliberately unchanged.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use balls::delivery_path::worktree_path;
use balls::layout::Xdg;
use tempfile::TempDir;

use crate::{delivery, git, project};

/// A §7 post wire (sealed id in `metadata`) carrying a delivery `target`.
fn post_into(invocation: &str, id: &str, title: &str, target: &str) -> String {
    format!(
        r#"{{"binding":{{"invocation_path":"{invocation}"}},"command":{{"op":"claim","target":"{target}"}},"current_state":{{"title":"{title}"}},"metadata":{{"bl-id":["{id}"]}}}}"#
    )
}

/// A §7 pre wire (the id comes off the change worktree) carrying a `target`.
fn pre_into(invocation: &str, title: &str, target: &str) -> String {
    format!(
        r#"{{"binding":{{"invocation_path":"{invocation}"}},"command":{{"op":"close","target":"{target}"}},"current_state":{{"title":"{title}"}}}}"#
    )
}

/// A §7 pre wire with NO target — the parentless case: deliver to integration.
fn pre_flat(invocation: &str, title: &str) -> String {
    format!(
        r#"{{"binding":{{"invocation_path":"{invocation}"}},"command":{{"op":"close"}},"current_state":{{"title":"{title}"}}}}"#
    )
}

/// A close.pre change worktree whose staged deletion of `tasks/<id>.md` is how
/// the pre hook recovers the id (the harness's own helper is fixed to `bl-x`).
fn change_for(tmp: &Path, name: &str, id: &str) -> PathBuf {
    let change = tmp.join(name);
    fs::create_dir(&change).unwrap();
    git(&change, &["init", "-q", "-b", "balls"]);
    git(&change, &["config", "user.name", "test"]);
    git(&change, &["config", "user.email", "test@example.com"]);
    fs::create_dir(change.join("tasks")).unwrap();
    fs::write(change.join("tasks").join(format!("{id}.md")), "x\n").unwrap();
    git(&change, &["add", "-A"]);
    git(&change, &["commit", "-qm", "seed"]);
    fs::remove_file(change.join("tasks").join(format!("{id}.md"))).unwrap();
    change
}

/// The tip subject of `rev` in the project repo at `root`.
fn subject(root: &Path, rev: &str) -> String {
    let out = Command::new("git").current_dir(root).args(["log", "-1", "--format=%s", rev]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn a_child_delivers_into_its_epics_ref_and_the_epic_lands_the_whole_thing_on_main() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));

    // The epic has no branch yet: the first child's claim MINTS `work/bl-epic`
    // at the integration head — a bare ref, no worktree, nothing to orphan —
    // and forks `work/bl-kid` off it.
    let kid = worktree_path(&xdg, "delivery", inv, "bl-kid");
    delivery(&root, &home, "claim", "post", &post_into(inv, "bl-kid", "Kid", "bl-epic")).assert().success();
    assert!(kid.join("seed.txt").exists());

    fs::write(kid.join("kid.txt"), "child work\n").unwrap();
    let change = change_for(tmp.path(), "change-kid", "bl-kid");
    delivery(&change, &home, "close", "pre", &pre_into(inv, "Kid", "bl-epic")).assert().success();
    delivery(&root, &home, "close", "post", &post_into(inv, "bl-kid", "Kid", "bl-epic")).assert().success();

    // Delivered, NOT landed: the squash sits on the epic's ref and `main` has
    // not moved. The child is done; the epic is what main is waiting on.
    assert_eq!(subject(&root, "work/bl-epic"), "Kid [bl-kid]");
    assert_eq!(subject(&root, "main"), "seed");
    assert!(!root.join("kid.txt").exists());

    // A second child accumulates onto the SAME ref — the epic is a thing in git
    // now, not a label on a report.
    let sib = worktree_path(&xdg, "delivery", inv, "bl-sib");
    delivery(&root, &home, "claim", "post", &post_into(inv, "bl-sib", "Sib", "bl-epic")).assert().success();
    assert!(sib.join("kid.txt").exists(), "a later child forks the epic's ref, seeing the earlier child's work");
    fs::write(sib.join("sib.txt"), "sibling work\n").unwrap();
    let change = change_for(tmp.path(), "change-sib", "bl-sib");
    delivery(&change, &home, "close", "pre", &pre_into(inv, "Sib", "bl-epic")).assert().success();
    assert_eq!(subject(&root, "work/bl-epic"), "Sib [bl-sib]");
    assert_eq!(subject(&root, "main"), "seed");

    // The epic itself is parentless — its target IS main — so its close folds
    // main in and lands both children as ONE commit. One reviewable unit.
    let change = change_for(tmp.path(), "change-epic", "bl-epic");
    delivery(&change, &home, "close", "pre", &pre_flat(inv, "The epic")).assert().success();
    assert_eq!(subject(&root, "main"), "The epic [bl-epic]");
    let files = String::from_utf8(
        Command::new("git").current_dir(&root).args(["ls-tree", "--name-only", "main"]).output().unwrap().stdout,
    )
    .unwrap();
    assert!(files.contains("kid.txt") && files.contains("sib.txt"), "both children landed with the epic: {files}");
}

#[test]
fn a_nested_close_runs_the_repos_own_pre_commit_gate_against_the_epics_ref() {
    // The hook is UNIFORM at every depth (bl-7b71): a child that breaks the
    // gate fails in ITS OWN worktree, at its own close, in front of the agent
    // that caused it — not deferred to whoever closes the epic last.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));

    let hooks = root.join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let kid = worktree_path(&xdg, "delivery", inv, "bl-kid");
    delivery(&root, &home, "claim", "post", &post_into(inv, "bl-kid", "Kid", "bl-epic")).assert().success();
    fs::write(kid.join("kid.txt"), "child work\n").unwrap();

    let change = change_for(tmp.path(), "change-kid", "bl-kid");
    delivery(&change, &home, "close", "pre", &pre_into(inv, "Kid", "bl-epic")).assert().failure();
    // Nothing delivered: the epic's ref still sits where it was minted.
    assert_eq!(subject(&root, "work/bl-epic"), "seed");
}
