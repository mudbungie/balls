//! §12 `prime` — the tracker's two readiness handlers under the sync loop, one
//! per axis of "make this checkout ready" (bl-0a23).
//!
//! - **`prime/pre` settles the NAME + clones the store in** ([`prime`]). With no
//!   remote it is STEALTH: touch no remote, persist nothing, say nothing (the
//!   expected first-run shape, bl-2013) — the
//!   opt-out is structural (no remote, nothing to leave on `origin`) and the
//!   DECLARED opt-out is a config fact core re-derives every op (the landing
//!   `task_remote` sentinel, bl-9df0), so there is no tracker-side state.
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
//!   the founding-miss is harmless and once-per-clone (§12) — and persists
//!   NOTHING: the miss is an outcome, re-derived per op, so re-running prime
//!   re-attempts by construction (bl-9df0). Established-vs-absent
//!   is read from the remote, never declared.

use super::git::git;
use super::payload::Binding;
use super::remote_ops::{not_yet_cut_over, remote_has_branch};
use super::Env;
use std::io;
use std::path::Path;

/// `prime/pre`: settle the store NAME and clone an established store in (§12).
/// Stealth (no remote) is SILENT and stops — persisting nothing; it is the
/// expected first-run shape and the declared opt-out already lives in config,
/// re-derivable via `bl conf` (bl-9df0/bl-2013). Otherwise warn on a
/// store-elsewhere mismatch and on an ephemeral remote (both diagnostic, never
/// fatal), then [`clone_in`] the
/// remote store branch if it is established and absent locally. Idempotent: a
/// re-prime finds the local branch present and clones nothing.
pub fn prime(b: &Binding, env: &Env) -> io::Result<()> {
    let Some(remote) = b.remote.clone() else {
        // Stealth (no remote): the EXPECTED first-run shape, so it stays SILENT —
        // narrating it every op was the wart (bl-2013). The DECLARED-vs-inferred
        // distinction it once drew is re-derivable on demand via `bl conf` (the
        // landing `task_remote` sentinel, bl-9df0), and the W2 ephemeral-remote
        // warning still covers the "forgot to federate via --remote" case.
        return Ok(());
    };
    let landing = Path::new(&b.landing);
    if let Some(durable) = ephemeral_gap(b, env, &remote) {
        eprintln!("tracker: primed on `{remote}` via an explicit remote; the durable ladder (XDG > origin) resolves {durable} — set `origin` or `bl conf set task-remote` to federate durably");
    }
    if let Some(named) = store_elsewhere(b, landing, &remote) {
        eprintln!("tracker: this repo's tasks are on `{named}` — run `bl install` / `bl prime --install`");
    }
    clone_in(landing, &remote, &b.tasks_branch)
}

/// The §12 ephemeral-remote gap (W2, bl-c2de): prime is acting on `remote`, but
/// the DURABLE ladder (landing `task_remote` > XDG `task-remote` > `origin`)
/// resolves to something else
/// — so it arrived via a per-op `--remote`/`--center` and plain commands will
/// not reproduce it (the bl-d234 silent-stealth failure). A landing stealth
/// sentinel is the strongest durable answer: plain commands run DECLARED
/// stealth, named as such (bl-9df0). Returns what durable
/// resolution yields, rendered for the warning; `None` = no gap (the remote in
/// use IS the durable one, however it was spelled).
fn ephemeral_gap(b: &Binding, env: &Env, remote: &str) -> Option<String> {
    let declared = crate::config::landing_remote(Path::new(&b.landing)).ok().flatten();
    if declared.is_some_and(|v| v == crate::config::STEALTH_REMOTE) {
        return Some("declared stealth (the landing `task_remote` sentinel)".to_string());
    }
    let durable = crate::config::xdg_remote(&env.xdg.user_config())
        .or_else(|| super::origin_of(Path::new(&b.invocation_path)));
    match durable {
        Some(d) if d == remote => None,
        Some(d) => Some(format!("`{d}`")),
        None => Some("nothing (plain commands run stealth)".to_string()),
    }
}

/// `prime/post`: settle the store CONTENT (§12). An ESTABLISHED remote branch is
/// brought current ([`super::remote_ops::sync`] — fetch + ff-only) then published
/// ([`super::remote_ops::push`] — a rejection is E5, the op aborts). An ABSENT
/// branch is FOUNDED by this push; a rejection there is the once-per-clone
/// founding-miss (no create perm) and degrades to stealth-local SILENTLY, the
/// fallback that is founding's ALONE (nothing existed to land on). Stealth (no
/// remote) no-ops, like every handler.
pub fn prime_post(b: &Binding) -> io::Result<()> {
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
    // stealth-local SILENTLY rather than error (§12) — persisting NOTHING: the
    // miss is an outcome, not a consent, so re-running prime re-attempts by
    // construction and a later op's push fails LOUDLY instead of never publishing
    // (bl-9df0); contrast the established push above, whose rejection is
    // split-brain.
    let _ = git(store, &["push", &remote, &b.tasks_branch]);
    Ok(())
}

/// Clone an established remote store branch into a LOCAL ref so core's
/// `materialize` checks it out — adopting an established history with no divergent
/// orphan to reconcile (bl-0a23, supersedes the bl-fa00 reset). Three cases, all
/// no-ops or one fetch: a local branch already here (a prior clone) is left for
/// `sync` to ff; an absent remote branch is the bootstrap, left for core to found;
/// only an established-remote-and-locally-absent branch is fetched, straight into
/// `refs/heads/<branch>` (the branch is checked out nowhere yet, so the refspec
/// just creates the ref). "Established" means an established STORE (bl-868d): a
/// remote tip with no `tasks/` — a hub still carrying the PRE-greenfield legacy
/// store on the colliding branch name (§16) — is QUARANTINED, not adopted
/// ([`not_yet_cut_over`] warns), so core founds a fresh greenfield orphan and the
/// runbook's "prime founds, import fills, cutover rewrites" holds on a fresh
/// clone. Runs against the LANDING — on a first prime the store is
/// not materialized yet, and landing + store share one object store and refs (§2).
fn clone_in(landing: &Path, remote: &str, branch: &str) -> io::Result<()> {
    if local_branch(landing, branch) {
        return Ok(()); // a prior clone — `sync` keeps it current
    }
    if !remote_has_branch(landing, remote, branch)? {
        return Ok(()); // bootstrap — nothing to clone; core founds, `prime/post` pushes
    }
    if not_yet_cut_over(landing, remote, branch) {
        return Ok(()); // a legacy tip is no store — leave it; core founds fresh (§16)
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

#[cfg(test)]
#[path = "prime_tests.rs"]
mod tests;
