//! §12/§13 remote ops: `sync` (import) and `push` (publish). The two halves of
//! "the store syncs every op" — pull the remote balls branch in before an op,
//! push the sealed result out after. Both are no-ops in a stealth (no-remote)
//! repo: with no remote there is nothing to talk to, which is the structural
//! opt-out (§12).

use super::git::git;
use super::payload::Binding;
use crate::safegit::reject_option_like;
use std::io;
use std::path::Path;

/// §13 `sync/pre`: import the remote balls branch by `fetch` + **fast-forward
/// only**. That one op is atomically detect-and-act — a non-ff IS the contention
/// signal (git's non-zero exit becomes ours: "remote wins, re-run"), so there is
/// no separate contention probe. Nothing is pushed, so a partial sync leaves
/// the store at the old or the new tip, never wedged (§13 rollback).
pub fn sync(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    let store = Path::new(&b.store);
    reject_option_like(remote)?;
    reject_option_like(&b.tasks_branch)?;
    git(store, &["fetch", remote, &b.tasks_branch])?;
    git(store, &["merge", "--ff-only", "FETCH_HEAD"])?;
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
    reject_option_like(remote)?;
    reject_option_like(&b.tasks_branch)?;
    git(Path::new(&b.store), &["push", remote, &b.tasks_branch])?;
    Ok(())
}

/// §6/§13 `install/pre`: fetch the center's config branch (`balls/config`,
/// [`crate::LANDING_BRANCH`]) into the LANDING repo so core can MATERIALIZE it
/// locally and copy it in. The tracker is balls' only remote-talker — core never
/// fetches (§0) — so `prime --install`'s remote read rides this hook. It leaves
/// the config at the landing's `FETCH_HEAD` (a git-standard ref, so no invented
/// core↔plugin convention); core reads it from the same checkout. This is a READ
/// only — config adoption is destructive on the LANDING, never a push to the
/// center (publishing is `install --to`, a separate direction). Stealth (no
/// remote) is a no-op, like every handler.
pub fn fetch_config(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    reject_option_like(remote)?;
    git(Path::new(&b.landing), &["fetch", remote, crate::LANDING_BRANCH])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::fixtures::{
        binding, checkout, commit, empty_remote, local_unpushed, store_clone,
        remote_with_branch, remote_with_config, tip, BRANCH,
    };
    use tempfile::TempDir;

    #[test]
    fn sync_fast_forwards_store_onto_the_advanced_remote() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        // A second checkout advances the remote out from under the store.
        let other = checkout(tmp.path(), &remote, "other");
        let moved = commit(&other, "next.txt", "next");
        git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

        sync(&binding(Some(&remote), &store)).unwrap();
        assert_eq!(tip(&store, "HEAD"), moved);
    }

    #[test]
    fn sync_in_stealth_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        let before = tip(&store, "HEAD");
        sync(&binding(None, &store)).unwrap();
        assert_eq!(tip(&store, "HEAD"), before);
    }

    #[test]
    fn sync_fails_on_a_non_fast_forward_the_contention_signal() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        // Diverge: a local commit AND a remote commit off the same base.
        commit(&store, "local.txt", "local");
        let other = checkout(tmp.path(), &remote, "other");
        commit(&other, "remote.txt", "remote");
        git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

        let err = sync(&binding(Some(&remote), &store)).unwrap_err();
        assert!(err.to_string().contains("git merge --ff-only"));
    }

    #[test]
    fn push_publishes_the_local_balls_branch_to_the_remote() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        let landed = commit(&store, "landed.txt", "landed");

        push(&binding(Some(&remote), &store)).unwrap();
        assert_eq!(tip(&remote, BRANCH), landed);
    }

    #[test]
    fn fetch_config_brings_the_centers_config_to_the_landing_fetch_head() {
        let tmp = TempDir::new().unwrap();
        let center = remote_with_config(tmp.path(), "balls/shared");
        let landing = local_unpushed(tmp.path()); // any local git repo to fetch into
        let mut b = binding(Some(&center), &landing);
        b.landing = landing.to_string_lossy().into_owned();
        fetch_config(&b).unwrap();
        // FETCH_HEAD in the landing now carries the center's config branch.
        let cfg = git(&landing, &["show", "FETCH_HEAD:config/balls.toml"]).unwrap();
        assert!(cfg.contains("balls/shared"), "fetched config: {cfg}");
    }

    #[test]
    fn fetch_config_in_stealth_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let landing = local_unpushed(tmp.path());
        let mut b = binding(None, &landing);
        b.landing = landing.to_string_lossy().into_owned();
        fetch_config(&b).unwrap(); // no remote → nothing fetched, no error
        assert!(git(&landing, &["rev-parse", "FETCH_HEAD"]).is_err());
    }

    #[test]
    fn push_in_stealth_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let remote = empty_remote(tmp.path());
        let store = local_unpushed(tmp.path());
        push(&binding(None, &store)).unwrap();
        // The empty remote still has no balls branch.
        assert!(git(&remote, &["rev-parse", BRANCH]).is_err());
    }

    #[test]
    fn push_fails_when_the_remote_rejects_a_non_fast_forward() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        // Remote moves ahead; the store's divergent commit can't ff-push.
        let other = checkout(tmp.path(), &remote, "other");
        commit(&other, "remote.txt", "remote");
        git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
        commit(&store, "local.txt", "local");

        assert!(push(&binding(Some(&remote), &store)).is_err());
    }

    #[test]
    fn sync_refuses_an_option_like_branch_before_touching_git() {
        // A config-sourced branch that begins with `-` (e.g. `--upload-pack=…`)
        // is refused as option-injection, not handed to `git fetch` (bl-2d6d).
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        let mut b = binding(Some(&remote), &store);
        b.tasks_branch = "--upload-pack=evil".into();
        let err = sync(&b).unwrap_err().to_string();
        assert!(err.contains("looks like an option"), "{err}");
    }
}
