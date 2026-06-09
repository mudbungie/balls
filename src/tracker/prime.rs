//! §12 `prime` — the tracker's two readiness handlers under the sync loop, one
//! per axis of "make this checkout ready" (bl-0a23).
//!
//! - **`prime/pre` settles the NAME + clones the store in** ([`prime`]). With no
//!   remote it is STEALTH: touch no remote, write a self-lock in this checkout's
//!   clone bundle (the opt-out is structural — no remote, nothing to leave on
//!   `origin`; installing the tracker with a remote IS the consent to federate).
//!   With a remote it (a) WARNS when the configured store sits elsewhere (a
//!   default-named clone of a repo whose canonical store is a non-default branch —
//!   diagnostic only; config crosses into a landing solely by `install`, §0/§12),
//!   and (b) CLONES IN: when the remote already carries the store branch and this
//!   clone has no local ref by that name yet, fetch it straight into a local
//!   branch so core's `materialize` CHECKS IT OUT (an established history adopts
//!   with no divergent orphan to reset — the bl-fa00 reset is gone). A local
//!   branch that already exists is left for `sync` to fast-forward; an absent
//!   remote branch is the bootstrap, left for core to found + `prime/post` to push.
//! - **`prime/post` settles the CONTENT** ([`prime_post`]). Established remote →
//!   fetch-ff (bring current) then push (publish); a rejected push to an
//!   ESTABLISHED store is split-brain and ERRORS (E5), never degrades. Absent
//!   remote branch → the founding push CREATES it; a rejection there (no create
//!   perm) falls back to stealth-local SILENTLY — nothing existed to land on, so
//!   the founding-miss is harmless and once-per-clone (§12). Established-vs-absent
//!   is read from the remote, never declared.

use super::git::git;
use super::payload::Binding;
use super::remote_ops::remote_has_branch;
use super::Env;
use std::fs;
use std::io;
use std::path::Path;

/// `prime/pre`: settle the store NAME and clone an established store in (§12).
/// Stealth (no remote) writes the self-lock and stops. Otherwise warn on a
/// store-elsewhere mismatch (diagnostic, never fatal), then [`clone_in`] the
/// remote store branch if it is established and absent locally. Idempotent: a
/// re-prime finds the local branch present and clones nothing.
pub fn prime(b: &Binding, env: &Env) -> io::Result<()> {
    let Some(remote) = b.remote.clone() else {
        return stealth_lock(b, env);
    };
    let landing = Path::new(&b.landing);
    if let Some(named) = store_elsewhere(b, landing, &remote) {
        eprintln!("tracker: this repo's tasks are on `{named}` — run `bl install` / `bl prime --install`");
    }
    clone_in(landing, &remote, &b.tasks_branch)
}

/// `prime/post`: settle the store CONTENT (§12). An ESTABLISHED remote branch is
/// brought current ([`super::remote_ops::sync`] — fetch + ff-only) then published
/// ([`super::remote_ops::push`] — a rejection is E5, the op aborts). An ABSENT
/// branch is FOUNDED by this push; a rejection there is the once-per-clone
/// founding-miss (no create perm) and degrades to stealth-local SILENTLY, the
/// fallback that is founding's ALONE (nothing existed to land on). Stealth (no
/// remote) no-ops, like every handler.
pub fn prime_post(b: &Binding, env: &Env) -> io::Result<()> {
    let Some(remote) = b.remote.clone() else {
        return Ok(());
    };
    let store = Path::new(&b.store);
    if remote_has_branch(store, &remote, &b.tasks_branch)? {
        super::remote_ops::sync(b)?; // established → bring current (fetch + ff-only)
        return super::remote_ops::push(b); // → publish; a reject is split-brain (E5)
    }
    // FOUNDING-MISS: the branch is absent, so this push CREATES it. A rejection is
    // the once-per-clone founding attempt failing for lack of a create permission
    // — harmless by definition (nothing existed to land on), so degrade to
    // stealth-local SILENTLY rather than error (§12); contrast the established push
    // above, whose rejection is split-brain.
    if git(store, &["push", &remote, &b.tasks_branch]).is_err() {
        return stealth_lock(b, env);
    }
    Ok(())
}

/// Clone an established remote store branch into a LOCAL ref so core's
/// `materialize` checks it out — adopting an established history with no divergent
/// orphan to reconcile (bl-0a23, supersedes the bl-fa00 reset). Three cases, all
/// no-ops or one fetch: a local branch already here (a prior clone) is left for
/// `sync` to ff; an absent remote branch is the bootstrap, left for core to found;
/// only an established-remote-and-locally-absent branch is fetched, straight into
/// `refs/heads/<branch>` (the branch is checked out nowhere yet, so the refspec
/// just creates the ref). Runs against the LANDING — on a first prime the store is
/// not materialized yet, and landing + store share one object store and refs (§2).
fn clone_in(landing: &Path, remote: &str, branch: &str) -> io::Result<()> {
    if local_branch(landing, branch) {
        return Ok(()); // a prior clone — `sync` keeps it current
    }
    if !remote_has_branch(landing, remote, branch)? {
        return Ok(()); // bootstrap — nothing to clone; core founds, `prime/post` pushes
    }
    git(landing, &["fetch", remote, &format!("{branch}:{branch}")])?;
    Ok(())
}

/// Does `repo` carry a local branch ref named `branch`? `show-ref --verify
/// --quiet` exits zero iff the ref resolves — the "already cloned in" signal.
fn local_branch(repo: &Path, branch: &str) -> bool {
    git(repo, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok()
}

/// The store this repo really uses, if it is NOT the one we are bound to — the
/// silent-empty diagnostic (§12). Returns `Some(branch)` only when our
/// `tasks_branch` is still the SEEDED DEFAULT (an un-`install`ed clone): it fetches
/// the standard `balls/config` landing branch from `remote` into `repo` (reading is
/// free, no authority) and reads its `tasks_branch`; a value DIFFERENT from ours is
/// the gap to warn about. Any failure to read it — remote unreachable, no
/// `balls/config` branch, malformed config — is UNCATCHABLE, silent by design:
/// `None`. A non-default name is a deliberate fork, never the gap.
fn store_elsewhere(b: &Binding, repo: &Path, remote: &str) -> Option<String> {
    if b.tasks_branch != crate::DEFAULT_TASKS_BRANCH {
        return None; // user-set or `install`-adopted — not the silent-empty gap
    }
    git(repo, &["fetch", remote, crate::LANDING_BRANCH]).ok()?;
    let cfg = git(repo, &["show", "FETCH_HEAD:config/balls.toml"]).ok()?;
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
#[path = "prime_tests.rs"]
mod tests;
