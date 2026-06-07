//! §12/§13 remote ops: `sync` (import) and `push` (publish). The two halves of
//! "the terminus syncs every op" — pull the remote balls branch in before an op,
//! push the sealed result out after. Both are no-ops in a stealth (no-remote)
//! repo: with no remote there is nothing to talk to, which is the structural
//! opt-out (§12).

use super::git::git;
use super::payload::Binding;
use std::io;
use std::path::Path;

/// §13 `sync/pre`: import the remote balls branch by `fetch` + **fast-forward
/// only**. That one op is atomically detect-and-act — a non-ff IS the contention
/// signal (git's non-zero exit becomes ours: "remote wins, re-run"), so there is
/// no separate contention probe. Nothing is pushed, so a partial sync leaves
/// `operating/` at the old or the new tip, never wedged (§13 rollback).
pub fn sync(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    let operating = Path::new(&b.store);
    git(operating, &["fetch", remote, &b.tasks_branch])?;
    git(operating, &["merge", "--ff-only", "FETCH_HEAD"])?;
    Ok(())
}

/// §12 `*/post`: publish the just-sealed balls branch to the remote — always to
/// an ESTABLISHED store (founding is `prime`'s alone, §12). A rejected push
/// (non-ff, perms revoked mid-life, a server-hook reject) means the mutation did
/// NOT land while the caller believes it is federated, so the non-zero exit
/// ABORTS the op (the pull → mutate → push contract) — it is NEVER silently
/// degraded to stealth, which would be split-brain (contrast `prime`'s
/// founding-miss fallback, where nothing existed to land on).
pub fn push(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    git(Path::new(&b.store), &["push", remote, &b.tasks_branch])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::fixtures::{
        binding, checkout, commit, empty_remote, local_unpushed, operating_clone,
        remote_with_branch, tip, BRANCH,
    };
    use tempfile::TempDir;

    #[test]
    fn sync_fast_forwards_operating_onto_the_advanced_remote() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let operating = operating_clone(tmp.path(), &remote);
        // A second checkout advances the remote out from under operating.
        let other = checkout(tmp.path(), &remote, "other");
        let moved = commit(&other, "next.txt", "next");
        git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

        sync(&binding(Some(&remote), &operating)).unwrap();
        assert_eq!(tip(&operating, "HEAD"), moved);
    }

    #[test]
    fn sync_in_stealth_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let operating = operating_clone(tmp.path(), &remote);
        let before = tip(&operating, "HEAD");
        sync(&binding(None, &operating)).unwrap();
        assert_eq!(tip(&operating, "HEAD"), before);
    }

    #[test]
    fn sync_fails_on_a_non_fast_forward_the_contention_signal() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let operating = operating_clone(tmp.path(), &remote);
        // Diverge: a local commit AND a remote commit off the same base.
        commit(&operating, "local.txt", "local");
        let other = checkout(tmp.path(), &remote, "other");
        commit(&other, "remote.txt", "remote");
        git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

        let err = sync(&binding(Some(&remote), &operating)).unwrap_err();
        assert!(err.to_string().contains("git merge --ff-only"));
    }

    #[test]
    fn push_publishes_the_local_balls_branch_to_the_remote() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let operating = operating_clone(tmp.path(), &remote);
        let landed = commit(&operating, "landed.txt", "landed");

        push(&binding(Some(&remote), &operating)).unwrap();
        assert_eq!(tip(&remote, BRANCH), landed);
    }

    #[test]
    fn push_in_stealth_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let remote = empty_remote(tmp.path());
        let operating = local_unpushed(tmp.path());
        push(&binding(None, &operating)).unwrap();
        // The empty remote still has no balls branch.
        assert!(git(&remote, &["rev-parse", BRANCH]).is_err());
    }

    #[test]
    fn push_fails_when_the_remote_rejects_a_non_fast_forward() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let operating = operating_clone(tmp.path(), &remote);
        // Remote moves ahead; operating's divergent commit can't ff-push.
        let other = checkout(tmp.path(), &remote, "other");
        commit(&other, "remote.txt", "remote");
        git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
        commit(&operating, "local.txt", "local");

        assert!(push(&binding(Some(&remote), &operating)).is_err());
    }
}
