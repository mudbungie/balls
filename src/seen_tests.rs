//! Tests for the bl-9f1d stale-read close guard — the CAS flow end to end
//! through [`crate::run`] (refuse-once-with-diff, bare-retry passes, show
//! acknowledges, prime sweeps) and the scope-by-home rungs on throwaway repos
//! (work-worktree mint, id-computed read union, the userspace-`.git` fence).

use super::*;
use crate::dispatch::support::{run_in, sole_task_id, store};
use tempfile::{tempdir, TempDir};

/// Init a plain repo at `dir` with a pinned commit identity.
fn repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    for args in [&["init", "-q", "-b", "main"][..], &["config", "user.name", "t"], &["config", "user.email", "t@e"]] {
        git::run(dir, args, None).unwrap();
    }
}

/// Commit `tasks/<id>.md` = `body` on `dir` with a §5 `bl-actor` trailer.
fn seal(dir: &Path, id: &str, body: &str, actor: &str) {
    fs::create_dir_all(dir.join("tasks")).unwrap();
    fs::write(dir.join("tasks").join(format!("{id}.md")), body).unwrap();
    git::run(dir, &["add", "-A"], None).unwrap();
    git::run(dir, &["commit", "-q", "-F", "-"], Some(&format!("t\n\nbl-actor: {actor}\n"))).unwrap();
}

/// A store-shaped repo whose `tasks/bl-7.md` was touched by `A` then amended
/// by `B` — the motivating race, in two commits.
fn raced_store(dir: &Path) {
    repo(dir);
    seal(dir, "bl-7", "claimed content\n", "A");
    seal(dir, "bl-7", "amended content\n", "B");
}

/// A project repo at `<tmp>/proj` with a `work/bl-7` LINKED worktree at
/// `<tmp>/bl-7` (the §11 shape: the worktree basename is the id).
fn project_with_worktree(tmp: &Path) -> (PathBuf, PathBuf) {
    let root = tmp.join("proj");
    repo(&root);
    fs::write(root.join("f"), "x").unwrap();
    git::run(&root, &["add", "-A"], None).unwrap();
    git::run(&root, &["commit", "-q", "-m", "seed"], None).unwrap();
    let wt = tmp.join("bl-7");
    git::run(&root, &["worktree", "add", "-q", "-b", "work/bl-7", &wt.to_string_lossy(), "main"], None).unwrap();
    (root, wt)
}

/// The store-clone token path for the full-run harness.
fn store_token(tmp: &TempDir, id: &str) -> PathBuf {
    gitdir(&store(tmp)).unwrap().join(TOKEN_DIR).join(id)
}

#[test]
fn a_foreign_edit_refuses_once_then_a_bare_retry_passes() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "a"]), 0);
    assert_eq!(run_in(&tmp, &["update", &id, "note=racing", "--as", "b"]), 0);
    // The unseen edit refuses the close — and the refusal minted the token.
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "a"]), 1);
    assert!(store_token(&tmp, &id).exists());
    // The bare retry seals exactly the acknowledged content and spends the token.
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "a"]), 0);
    assert!(!store_token(&tmp, &id).exists());
    assert!(!store(&tmp).join("tasks").join(format!("{id}.md")).exists());
}

#[test]
fn an_edit_landing_between_refusal_and_retry_refuses_again() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "a"]), 0);
    assert_eq!(run_in(&tmp, &["update", &id, "note=one", "--as", "b"]), 0);
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "a"]), 1);
    // Yet another edit lands: the stale token must NOT pass the next close.
    assert_eq!(run_in(&tmp, &["update", &id, "note=two", "--as", "b"]), 0);
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "a"]), 1);
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "a"]), 0);
}

#[test]
fn a_self_edit_never_refuses_and_an_unraced_close_is_frictionless() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "a"]), 0);
    // The closer's own update IS their last touch — zero friction (bl-9f1d).
    assert_eq!(run_in(&tmp, &["update", &id, "note=mine", "--as", "a"]), 0);
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "a"]), 0);
}

#[test]
fn show_acknowledges_the_pending_edit_for_the_store_scope() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "a"]), 0);
    assert_eq!(run_in(&tmp, &["update", &id, "note=racing", "--as", "b"]), 0);
    // The diff entered a stdout at this invocation path: close passes first try.
    assert_eq!(run_in(&tmp, &["show", &id]), 0);
    assert!(store_token(&tmp, &id).exists());
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "a"]), 0);
    assert!(!store_token(&tmp, &id).exists()); // consumed by the seal
}

#[test]
fn a_dead_ball_show_mints_nothing_and_prime_sweeps_dead_tokens() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["show", &id]), 0); // the dead render
    assert!(!store_token(&tmp, &id).exists()); // nothing at the tip to acknowledge
    // A live ball's token survives the sweep; a dead one's is debris.
    assert_eq!(run_in(&tmp, &["create", "B task", "--as", "me"]), 0);
    let live = sole_task_id(&store(&tmp).join("tasks"));
    assert_eq!(run_in(&tmp, &["show", &live]), 0);
    fs::write(store_token(&tmp, &id).parent().unwrap().join("bl-gone"), "feed\n").unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert!(store_token(&tmp, &live).exists());
    assert!(!store_token(&tmp, "bl-gone").exists());
}

#[test]
fn a_closer_who_never_touched_the_ball_sees_the_whole_file_once() {
    let d = tempdir().unwrap();
    let s = d.path().join("store");
    repo(&s);
    seal(&s, "bl-7", "whole content\n", "A");
    // No anchor for C: the diff bases on the empty tree — everything is unseen.
    let err = guard(&s, d.path(), "bl-7", "C").unwrap_err().to_string();
    assert!(err.contains("changed since your last touch"), "{err}");
    assert!(err.contains("+whole content"), "{err}");
    // The refusal minted the acknowledgment: the retry passes and consumes it.
    let tokens = guard(&s, d.path(), "bl-7", "C").unwrap();
    assert_eq!(tokens.len(), 1);
    consume(&tokens);
    assert!(!tokens[0].exists());
}

#[test]
fn guard_refuses_a_store_whose_task_file_is_not_at_the_tip() {
    let d = tempdir().unwrap();
    let s = d.path().join("store");
    repo(&s);
    git::run(&s, &["commit", "-q", "--allow-empty", "-m", "root"], None).unwrap();
    fs::create_dir_all(s.join("tasks")).unwrap();
    fs::write(s.join("tasks/bl-7.md"), "uncommitted\n").unwrap();
    let err = guard(&s, d.path(), "bl-7", "A").unwrap_err().to_string();
    assert!(err.contains("not at the store tip"), "{err}");
}

#[test]
fn standing_in_a_work_worktree_mints_into_that_worktrees_own_gitdir() {
    let d = tempdir().unwrap();
    let s = d.path().join("store");
    repo(&s);
    seal(&s, "bl-7", "content\n", "A");
    let (root, wt) = project_with_worktree(d.path());
    mint(&wt, &s, "bl-7");
    let token = root.join(".git/worktrees/bl-7").join(TOKEN_DIR).join("bl-7");
    assert!(token.exists(), "per-agent scope: the mint home is the worktree gitdir");
    assert!(!gitdir(&s).unwrap().join(TOKEN_DIR).join("bl-7").exists());
}

#[test]
fn a_worktree_minted_token_is_read_from_the_repo_root_by_id() {
    let d = tempdir().unwrap();
    let s = d.path().join("store");
    raced_store(&s);
    let (root, wt) = project_with_worktree(d.path());
    // The claimant saw the amendment from INSIDE their worktree; the close runs
    // from the repo root — the union computes the worktree rung from the id.
    mint(&wt, &s, "bl-7");
    let tokens = guard(&s, &root, "bl-7", "A").unwrap();
    assert_eq!(tokens, vec![root.join(".git/worktrees/bl-7").join(TOKEN_DIR).join("bl-7")]);
}

#[test]
fn the_union_dedups_when_standing_in_the_tasks_own_worktree() {
    let d = tempdir().unwrap();
    let s = d.path().join("store");
    repo(&s);
    seal(&s, "bl-7", "content\n", "A");
    let (root, wt) = project_with_worktree(d.path());
    // Current-worktree rung == the id-computed rung: one entry, not two.
    let homes = union(&s, &wt, "bl-7").unwrap();
    assert_eq!(homes, vec![gitdir(&s).unwrap(), root.join(".git/worktrees/bl-7")]);
}

#[test]
fn a_work_branch_at_the_repo_root_is_not_bl_territory() {
    let d = tempdir().unwrap();
    let s = d.path().join("store");
    repo(&s);
    seal(&s, "bl-7", "content\n", "A");
    let root = d.path().join("proj");
    repo(&root);
    fs::write(root.join("f"), "x").unwrap();
    git::run(&root, &["add", "-A"], None).unwrap();
    git::run(&root, &["commit", "-q", "-m", "seed"], None).unwrap();
    // A `work/` branch checked out at the ROOT: gitdir == common dir, so the
    // mint falls to the store rung — bl never writes the userspace `.git`.
    git::run(&root, &["switch", "-q", "-c", "work/bl-7"], None).unwrap();
    mint(&root, &s, "bl-7");
    assert!(!root.join(".git").join(TOKEN_DIR).join("bl-7").exists());
    assert!(gitdir(&s).unwrap().join(TOKEN_DIR).join("bl-7").exists());
}

#[test]
fn gitdir_refuses_a_non_repo_store() {
    let d = tempdir().unwrap();
    let err = gitdir(d.path()).unwrap_err().to_string();
    assert!(err.contains("not a git checkout"), "{err}");
}
