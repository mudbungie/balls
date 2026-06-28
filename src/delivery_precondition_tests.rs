//! Unit tests for the §11 delivery precondition (bl-4a88): the [`require_repo`]
//! gate matrix against a fake [`Repo`] whose `is_git_repo` is dialable, and the
//! shared [`precondition_unmet`] message. The real predicate ([`Project::
//! is_git_repo`]) is covered against actual temp dirs in `delivery_repo_tests`.

use super::*;
use std::path::Path;

/// A [`Repo`] that answers only [`Repo::is_git_repo`] (the one act the gate
/// consults); every other method is unreachable on the gate's paths.
struct FakeRepo {
    is_repo: bool,
}

impl Repo for FakeRepo {
    fn is_git_repo(&self) -> io::Result<bool> {
        Ok(self.is_repo)
    }
    fn materialize(&self, _: &Path, _: &str) -> io::Result<()> {
        unreachable!("the gate consults only is_git_repo")
    }
    fn release(&self, _: &Path) -> io::Result<()> {
        unreachable!("the gate consults only is_git_repo")
    }
    fn discard(&self, _: &Path, _: &str) -> io::Result<()> {
        unreachable!("the gate consults only is_git_repo")
    }
    fn integration(&self) -> io::Result<String> {
        unreachable!("the gate consults only is_git_repo")
    }
    fn deliver(&self, _: &Path, _: &str, _: &str, _: &str, _: &str) -> io::Result<()> {
        unreachable!("the gate consults only is_git_repo")
    }
    fn work_messages(&self, _: &str, _: &str) -> io::Result<Vec<String>> {
        unreachable!("the gate consults only is_git_repo")
    }
}

fn gate(op: &str, phase: &str, rolling_back: bool, is_repo: bool) -> io::Result<()> {
    require_repo(op, phase, rolling_back, &FakeRepo { is_repo }, "/proj")
}

#[test]
fn claim_post_and_close_pre_abort_when_root_is_not_a_repo() {
    for (op, phase) in [("claim", "post"), ("close", "pre")] {
        let err = gate(op, phase, false, false).unwrap_err();
        let msg = err.to_string();
        // The clean balls-voice message, NOT git's raw fatal — naming both drains.
        assert!(msg.contains("/proj is not a git repository"), "{op}.{phase}: {msg}");
        assert!(msg.contains("git init"), "{op}.{phase}: {msg}");
        assert!(msg.contains("bl conf remove"), "{op}.{phase}: {msg}");
        assert!(!msg.contains("fatal:"), "{op}.{phase}: {msg}");
    }
}

#[test]
fn the_gated_ops_pass_when_root_is_a_repo() {
    // A bare or normal repo (is_git_repo true) → the precondition holds, the act
    // proceeds (the gate returns Ok without ever touching the worktree methods).
    assert!(gate("claim", "post", false, true).is_ok());
    assert!(gate("close", "pre", false, true).is_ok());
}

#[test]
fn every_other_op_phase_is_ungated_and_never_consults_the_repo() {
    // The predicate is not even evaluated here — a non-repo passes — so the
    // path-guarded teardowns/reads and a rollback of a gated op never abort.
    for (op, phase, rb) in [
        ("unclaim", "post", false), // teardown — release is path-guarded
        ("close", "post", false),   // teardown
        ("claim", "pre", false),    // wrong phase
        ("show", "read", false),    // a read
        ("claim", "post", true),    // a ROLLBACK of claim.post (discard) — not gated
        ("close", "pre", true),     // a rollback of close.pre — not gated
    ] {
        assert!(gate(op, phase, rb, false).is_ok(), "{op}.{phase} rb={rb} must be ungated");
    }
}
