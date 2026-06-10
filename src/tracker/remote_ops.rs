//! §12/§13 remote ops: `sync` (import) and `push` (publish). `sync` imports on
//! the explicit `bl sync` (and inside prime); `push` publishes after every
//! mutating op. Currency is OPTIMISTIC (mutate → push, bl-336a): there is no
//! pre-pull — a stale store surfaces atomically as the push's non-ff reject
//! (E5), and recovery is `bl sync` + retry. Both are no-ops in a stealth
//! (no-remote) repo: with no remote there is nothing to talk to, which is the
//! structural opt-out (§12).

use super::git::git;
use super::payload::Binding;
use crate::safegit::reject_option_like;
use std::io;
use std::path::Path;

/// §13 `sync/pre`: the general rule — fetch the branch's UPSTREAM, **if any**,
/// then **fast-forward** THAT branch. "If any" is read from the remote
/// ([`remote_has_branch`], the same ls-remote that decides prime's
/// adopt-vs-found): an upstream-less branch — the landing by construction (§4),
/// any local-only branch — yields a no-op *for free*, no name special-cased.
/// The ff target is the branch the binding NAMES, never whatever the store
/// checkout happens to have checked out: the store's own branch integrates by
/// `merge --ff-only FETCH_HEAD` (the working tree moves with it); any other
/// branch is a pure ref move via the `<branch>:<branch>` refspec (ff-only by
/// git's own default). Either way the ff is atomically detect-and-act — a
/// non-ff IS the contention signal (git's non-zero exit becomes ours: "remote
/// wins, re-run"), so there is no separate contention probe. Nothing is pushed,
/// so a partial sync leaves the branch at the old or the new tip, never wedged
/// (§13 rollback).
pub fn sync(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    let store = Path::new(&b.store);
    let branch = b.tasks_branch.as_str();
    reject_option_like(remote)?;
    reject_option_like(branch)?;
    if !remote_has_branch(store, remote, branch)? {
        return Ok(()); // no upstream — the §13 no-op, for free
    }
    if git(store, &["symbolic-ref", "--short", "HEAD"]).ok().as_deref() == Some(branch) {
        git(store, &["fetch", remote, branch])?;
        git(store, &["merge", "--ff-only", "FETCH_HEAD"])?;
    } else {
        git(store, &["fetch", remote, &format!("{branch}:{branch}")])?;
    }
    Ok(())
}

/// Does `remote` already carry `branch`? `git ls-remote --heads` is the one
/// round-trip that answers "an upstream, if any" — sync's no-op gate and
/// prime's adopt-vs-found / clone-vs-bootstrap signal (§12/§13).
pub(super) fn remote_has_branch(cwd: &Path, remote: &str, branch: &str) -> io::Result<bool> {
    Ok(!git(cwd, &["ls-remote", "--heads", remote, branch])?.is_empty())
}

/// §12 `*/post`: publish the just-sealed balls branch to the remote — always to
/// an ESTABLISHED store (founding is `prime`'s alone, §12). A rejected push
/// (non-ff, perms revoked mid-life, a server-hook reject) means the mutation did
/// NOT land while the caller believes it is federated, so the non-zero exit
/// ABORTS the op (the push IS the optimistic mutate → push contention check;
/// re-run after `bl sync`) — it is NEVER silently degraded to stealth, which
/// would be split-brain (contrast `prime`'s founding-miss fallback, where
/// nothing existed to land on).
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
/// remote) is a no-op, like every handler — and so is a present remote that
/// simply LACKS the ref (bl-45fd): bl never publishes the landing (§4
/// single-owner), so a stock hub carries no `balls/config`, and a purely local
/// install must not depend on remote state. The gate is sync's own
/// [`remote_has_branch`] ("an upstream, if any", §13); an adopt that really
/// needs the center's config fails at point-of-use (no `FETCH_HEAD`).
pub fn fetch_config(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    reject_option_like(remote)?;
    let landing = Path::new(&b.landing);
    if !remote_has_branch(landing, remote, crate::LANDING_BRANCH)? {
        return Ok(()); // no landing on the hub — the §13 no-op, for free
    }
    git(landing, &["fetch", remote, crate::LANDING_BRANCH])?;
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
    fn sync_of_an_upstream_less_branch_is_a_no_op_the_landing_for_free() {
        // §13: "fetch a branch's upstream, if any" — the remote carries no
        // `balls/config`, so syncing the landing BY ITS REAL NAME fetches
        // nothing and ff's nothing. No token is special-cased; any local-only
        // branch takes the same no-op path.
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        let before = tip(&store, "HEAD");
        for upstream_less in [crate::LANDING_BRANCH, "work/bl-0000"] {
            let mut b = binding(Some(&remote), &store);
            b.tasks_branch = upstream_less.into();
            sync(&b).unwrap();
            assert_eq!(tip(&store, "HEAD"), before);
        }
    }

    #[test]
    fn sync_of_a_non_checked_out_branch_ffs_that_branch_not_the_checkout() {
        // §13: the ff target is the branch the binding NAMES. The store sits on
        // `balls`; syncing `other` moves refs/heads/other (a pure ref move) and
        // leaves the checked-out branch where it was.
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let store = store_clone(tmp.path(), &remote);
        let seat = checkout(tmp.path(), &remote, "seat");
        git(&seat, &["checkout", "-q", "-b", "other"]).unwrap();
        let moved = commit(&seat, "other.txt", "other");
        git(&seat, &["push", "-q", "origin", "other"]).unwrap();

        let head = tip(&store, "HEAD");
        let mut b = binding(Some(&remote), &store);
        b.tasks_branch = "other".into();
        sync(&b).unwrap();
        assert_eq!(tip(&store, "other"), moved); // the named branch moved…
        assert_eq!(tip(&store, "HEAD"), head); // …the checkout did not
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
    fn fetch_config_when_the_remote_lacks_the_landing_is_a_no_op() {
        // bl-45fd: the landing is never pushed by bl (§4 single-owner), so a
        // stock hub carries no `balls/config`. A present remote MISSING the
        // ref is §13's "upstream, if any" no-op — not a fatal abort of a
        // purely local install. Only an adopt naming the center as --from
        // needs the fetch, and that fails at point-of-use (no FETCH_HEAD).
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path()); // carries `balls`, no `balls/config`
        let landing = local_unpushed(tmp.path());
        let mut b = binding(Some(&remote), &landing);
        b.landing = landing.to_string_lossy().into_owned();
        fetch_config(&b).unwrap(); // ref absent → nothing fetched, no error
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
