//! §12 `prime` — the tracker's readiness handler under the sync loop.
//!
//! Two paths, read from state, never from a flag:
//! - **Stealth** (no remote resolved): touch no remote and write a self-lock in
//!   this checkout's clone bundle. The opt-out is structural — with no remote
//!   there is nothing to leave on `origin`; installing the tracker (and giving
//!   it a remote) IS the consent to federate. The lock marks the landing locked
//!   so a later prime does not silently auto-extend it (notice W1).
//! - **Tracked**: the upstream is the config-named store remote
//!   (`binding.remote`, §12) — read DIRECTLY, with no trail to walk (config
//!   crosses a checkout boundary exactly once, by `install`, §4/§12). Then
//!   **adopt or found**, the one sync-or-bootstrap step: an established remote
//!   store branch is left for `sync` to keep current (adopt); an ABSENT one is
//!   founded by pushing the local checkout (bootstrap-on-miss).
//!   Established-vs-absent is read from the remote, not declared.

use super::git::git;
use super::payload::Binding;
use super::Env;
use std::fs;
use std::io;
use std::path::Path;

/// Bring the tracker to readiness for `b` (§12). Idempotent: a re-run founds
/// nothing new (the branch now exists → adopt) and re-locks an already-locked
/// stealth checkout, so it converges to a no-op.
pub fn prime(b: &Binding, env: &Env) -> io::Result<()> {
    let Some(remote) = b.remote.clone() else {
        return stealth_lock(b, env);
    };
    let store = Path::new(&b.store);
    if !remote_has_branch(store, &remote, &b.tasks_branch)? {
        git(store, &["push", &remote, &b.tasks_branch])?; // found the remote
    }
    Ok(())
}

/// Does `remote` already carry `branch`? `git ls-remote --heads` is the one
/// round-trip that decides adopt vs found.
fn remote_has_branch(cwd: &Path, remote: &str, branch: &str) -> io::Result<bool> {
    Ok(!git(cwd, &["ls-remote", "--heads", remote, branch])?.is_empty())
}

/// Write the self-reference stealth lock into this checkout's clone bundle (§1).
fn stealth_lock(b: &Binding, env: &Env) -> io::Result<()> {
    let bundle = env.xdg.clone_dir(Path::new(&b.invocation_path));
    fs::create_dir_all(bundle.root())?;
    fs::write(bundle.root().join("stealth.lock"), "stealth: no remote\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::fixtures::{
        binding, empty_remote, env, local_unpushed, operating_clone, remote_with_branch, tip,
        BRANCH,
    };
    use tempfile::TempDir;

    #[test]
    fn stealth_writes_a_self_lock_and_touches_no_remote() {
        let tmp = TempDir::new().unwrap();
        let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
        let b = binding(None, &tmp.path().join("landing"));
        prime(&b, &env).unwrap();
        let lock = env
            .xdg
            .clone_dir(Path::new(&b.invocation_path))
            .root()
            .join("stealth.lock");
        assert!(lock.is_file());
    }

    #[test]
    fn an_absent_remote_branch_is_founded() {
        let tmp = TempDir::new().unwrap();
        let remote = empty_remote(tmp.path());
        let operating = local_unpushed(tmp.path());
        let env = env(&tmp.path().join("home"), &tmp.path().join("state"));

        prime(&binding(Some(&remote), &operating), &env).unwrap();
        assert_eq!(tip(&remote, BRANCH), tip(&operating, "HEAD"));
    }

    #[test]
    fn an_established_remote_is_adopted_not_re_pushed() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_branch(tmp.path());
        let operating = operating_clone(tmp.path(), &remote);
        let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
        let before = tip(&remote, BRANCH);

        prime(&binding(Some(&remote), &operating), &env).unwrap();
        assert_eq!(tip(&remote, BRANCH), before); // no push happened
    }
}
