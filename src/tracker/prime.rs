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
//!   founded by pushing the local checkout (bootstrap-on-miss). A founding push
//!   REJECTED for lack of a create permission falls back to stealth-local
//!   silently — harmless by definition, since nothing existed to land on (§12).
//!   That silent fallback is founding's ALONE: a rejected push to an ESTABLISHED
//!   store ([`super::remote_ops::push`], every op's post) ERRORS (E5), never
//!   degrades to stealth — a believed-federated mutation that vanished is
//!   split-brain. Established-vs-absent is read from the remote, not declared.

use super::git::git;
use super::payload::Binding;
use super::Env;
use std::fs;
use std::io;
use std::path::Path;

/// Bring the tracker to readiness for `b` (§12). Idempotent: a re-run founds
/// nothing new (the branch now exists → adopt) and re-locks an already-locked
/// stealth checkout, so it converges to a no-op. A founding push the remote
/// rejects (no create perm) degrades to stealth — the founding-miss case, kept
/// distinct from an established-store push reject, which errors (`remote_ops`).
pub fn prime(b: &Binding, env: &Env) -> io::Result<()> {
    let Some(remote) = b.remote.clone() else {
        return stealth_lock(b, env);
    };
    let store = Path::new(&b.store);
    if let Some(named) = store_elsewhere(b, store, &remote) {
        eprintln!("tracker: this repo's tasks are on `{named}` — run `bl install` / `bl prime --install`");
    }
    if remote_has_branch(store, &remote, &b.tasks_branch)? {
        return Ok(()); // established → adopt; `sync` keeps it current
    }
    // FOUNDING-MISS: the branch is absent, so this push CREATES it. A rejection
    // here is the once-per-clone founding attempt failing for lack of a create
    // permission — harmless by definition (nothing existed to land on), so we
    // degrade to stealth-local SILENTLY rather than error (§12). This silent
    // fallback is founding's ALONE: an established-store push (every op's
    // `*/post`, `remote_ops::push`) ERRORS on rejection (E5), because there a
    // believed-federated mutation did NOT land — silently degrading would be
    // split-brain.
    if git(store, &["push", &remote, &b.tasks_branch]).is_err() {
        return stealth_lock(b, env);
    }
    Ok(())
}

/// Does `remote` already carry `branch`? `git ls-remote --heads` is the one
/// round-trip that decides adopt vs found.
fn remote_has_branch(cwd: &Path, remote: &str, branch: &str) -> io::Result<bool> {
    Ok(!git(cwd, &["ls-remote", "--heads", remote, branch])?.is_empty())
}

/// The store this repo really uses, if it is NOT the one we are bound to — the
/// silent-empty diagnostic (§12). Returns `Some(branch)` only when our
/// `tasks_branch` is still the SEEDED DEFAULT (an un-`install`ed clone): it fetches
/// the standard `balls/config` landing branch from `remote` (reading is free, no
/// authority) and reads its `tasks_branch`; a value DIFFERENT from ours is the gap
/// to warn about. Any failure to read it — remote unreachable, no `balls/config`
/// branch, malformed config — is an UNCATCHABLE case, silent by design: `None`.
fn store_elsewhere(b: &Binding, store: &Path, remote: &str) -> Option<String> {
    if b.tasks_branch != crate::DEFAULT_TASKS_BRANCH {
        return None; // user-set or `install`-adopted — not the silent-empty gap
    }
    git(store, &["fetch", remote, crate::LANDING_BRANCH]).ok()?;
    let cfg = git(store, &["show", "FETCH_HEAD:config/balls.toml"]).ok()?;
    let named = toml::from_str::<toml::Table>(&cfg)
        .ok()?
        .get("tasks_branch")?
        .as_str()?
        .to_string();
    (named != b.tasks_branch).then_some(named)
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
        binding, default_binding, empty_remote, env, local_unpushed, operating_clone,
        remote_with_branch, remote_with_config, tip, unpushable_remote, BRANCH,
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
    fn a_rejected_push_falls_back_to_stealth_local() {
        let tmp = TempDir::new().unwrap();
        let remote = unpushable_remote(tmp.path());
        let operating = local_unpushed(tmp.path());
        let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
        let b = binding(Some(&remote), &operating);

        prime(&b, &env).unwrap(); // push denied → silent stealth fallback, not an error
        let lock = env.xdg.clone_dir(Path::new(&b.invocation_path)).root().join("stealth.lock");
        assert!(lock.is_file()); // fell back to stealth-local
        assert!(git(&remote, &["rev-parse", BRANCH]).is_err()); // nothing was founded
    }

    #[test]
    fn store_elsewhere_reports_a_differently_configured_origin() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_config(tmp.path(), "balls/work");
        let store = local_unpushed(tmp.path());
        let b = default_binding(Some(&remote), &store);
        // The synced origin:balls/config names a different store than our default.
        assert_eq!(
            store_elsewhere(&b, Path::new(&b.store), b.remote.as_deref().unwrap()),
            Some("balls/work".to_string())
        );
    }

    #[test]
    fn store_elsewhere_is_silent_when_origin_agrees_or_is_unreadable() {
        let tmp = TempDir::new().unwrap();
        let store = local_unpushed(tmp.path());
        // Origin's config names the SAME default store → no gap.
        let agree = remote_with_config(tmp.path(), crate::DEFAULT_TASKS_BRANCH);
        let b = default_binding(Some(&agree), &store);
        assert_eq!(store_elsewhere(&b, Path::new(&b.store), b.remote.as_deref().unwrap()), None);
        // No balls/config branch to read → uncatchable, silent.
        let bare = empty_remote(tmp.path());
        let b = default_binding(Some(&bare), &store);
        assert_eq!(store_elsewhere(&b, Path::new(&b.store), b.remote.as_deref().unwrap()), None);
    }

    #[test]
    fn prime_warns_about_a_relocated_store_but_does_not_abort() {
        let tmp = TempDir::new().unwrap();
        let remote = remote_with_config(tmp.path(), "balls/work");
        let store = local_unpushed(tmp.path());
        let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
        // Default tasks_branch + a relocated origin: prime warns (stderr) then,
        // finding no default store to push (local has no such branch), falls back
        // to stealth — the warning is diagnostic, never fatal.
        prime(&default_binding(Some(&remote), &store), &env).unwrap();
    }
}
