//! §12 `prime` — the tracker's readiness handler under the sync loop.
//!
//! Two paths, read from state, never from a flag:
//! - **Stealth** (no remote resolved): touch no remote and write a self-lock in
//!   this checkout's clone bundle. The opt-out is structural — with no remote
//!   there is nothing to leave on `origin`; installing the tracker (and giving
//!   it a remote) IS the consent to federate. The lock marks the landing locked
//!   so a later prime does not silently auto-extend it (notice W1).
//! - **Tracked**: resolve the upstream — the committed trail pointer (§12) wins
//!   over the auto-discovered wire remote — then **adopt or found**, the one
//!   sync-or-bootstrap step. An established remote balls branch is left for
//!   `sync` to keep current (adopt); an ABSENT one is founded by pushing the
//!   local checkout (bootstrap-on-miss). Established-vs-absent is read from the
//!   remote, not declared.

use super::git::git;
use super::payload::Binding;
use super::{pointer, Env};
use std::fs;
use std::io;
use std::path::Path;

/// Bring the tracker to readiness for `b` (§12). Idempotent: a re-run founds
/// nothing new (the branch now exists → adopt) and re-locks an already-locked
/// stealth checkout, so it converges to a no-op.
pub fn prime(b: &Binding, env: &Env) -> io::Result<()> {
    let Some(remote) = resolve(b)? else {
        return stealth_lock(b, env);
    };
    let operating = Path::new(&b.operating);
    if !remote_has_branch(operating, &remote, &b.branch)? {
        git(operating, &["push", &remote, &b.branch])?; // found the remote
    }
    Ok(())
}

/// The upstream to ready: the committed trail pointer (`next:`) if set, else the
/// auto-discovered wire remote. The pointer winning is what lets a fresh clone
/// reach a central store it could not name directly (§12).
fn resolve(b: &Binding) -> io::Result<Option<String>> {
    match pointer::read(Path::new(&b.operating))? {
        Some(next) => Ok(Some(next)),
        None => Ok(b.remote.clone()),
    }
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
        binding, empty_remote, env, local_unpushed, operating_clone, remote_with_branch,
        set_pointer, tip, BRANCH,
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

    #[test]
    fn the_committed_pointer_overrides_the_wire_remote() {
        let tmp = TempDir::new().unwrap();
        let wire = remote_with_branch(tmp.path()); // the auto-discovered remote
        let pointed = empty_remote(tmp.path()); // the committed next: hop, absent
        let operating = local_unpushed(tmp.path());
        set_pointer(&operating, &pointed.to_string_lossy());
        let env = env(&tmp.path().join("home"), &tmp.path().join("state"));

        // Resolve picks `pointed`, finds it absent, and founds it — not `wire`.
        prime(&binding(Some(&wire), &operating), &env).unwrap();
        assert_eq!(tip(&pointed, BRANCH), tip(&operating, "HEAD"));
    }
}
