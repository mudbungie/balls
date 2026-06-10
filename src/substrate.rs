//! §12 substrate — `prime`'s bootstrap-on-miss, the retired `init`.
//!
//! Founding is not a separate verb: it is the local-miss branch of idempotent
//! `prime` (§12). The two-branch substrate (§2) is founded in TWO steps, on two
//! different schedules:
//! - [`found_landing`] lays the **landing** (`balls/config`, holding `config/`)
//!   as the repo's first worktree EAGERLY — `prime` needs its `config/` to know
//!   the plugin chain and the configured `tasks_branch` before it can do anything.
//! - [`materialize`] lays the **store** (`tasks_branch`, holding `tasks/`)
//!   LAZILY, between `prime`'s pre and post phases (bl-0a23): it checks out the branch if
//!   a ref already exists — a remote one the `prime/pre` tracker just cloned in
//!   (§12) — and founds a fresh orphan ONLY when no such ref exists (the genuine
//!   no-remote bootstrap). Founding eagerly would create a divergent orphan that
//!   an established remote could not fast-forward onto, the unrelated-histories
//!   bug bl-fa00 had to reset away; materializing after the tracker's clone-in
//!   means that divergence is never CREATED.
//!
//! One repo, two branches, two real checkouts — no symlink indirection, no chain
//! to resolve (§1). Core knows nothing of remotes here (§0); it only ensures the
//! two checkouts exist and seeds the landing's `config/` from the app
//! default-config ([`crate::seed`]). Re-running `prime` skips both steps (the
//! landing is already a landing, the store checkout already sits on the
//! configured `tasks_branch` — the §12 predicate, not "a store dir exists",
//! bl-eb52), so the whole verb converges to a no-op — there is no `--reinit`.

use crate::git;
use crate::layout::Xdg;
use crate::seed;
use crate::LANDING_BRANCH;
use std::fs;
use std::io;
use std::path::Path;

/// Found the landing half of the substrate (§2 bootstrap-on-miss): the
/// `balls/config` branch at `landing`, its `config/` SEEDED from the app
/// default-config (the `balls.toml` + the `plugins.toml` hook schedule, with each
/// named plugin found beside `bl` in `exe_dir` bound and every absent-binary entry
/// pruned, §12). The caller guarantees the landing does not already exist, so this
/// never clobbers an established checkout. The STORE is NOT founded here — that is
/// [`materialize`]'s lazy job, run after the tracker's `prime/pre` has
/// had its chance to clone an established remote branch in (bl-0a23).
pub fn found_landing(landing: &Path, xdg: &Xdg, exe_dir: Option<&Path>) -> io::Result<()> {
    fs::create_dir_all(landing)?;
    git::run(landing, &["init", "-q", "-b", LANDING_BRANCH], None)?;
    identify(landing)?;
    fs::write(landing.join(".gitignore"), "/config/plugins/bin/\n")?;
    seed::seed_landing(xdg, landing, exe_dir)?;
    git::run(landing, &["add", "-A"], None)?;
    git::run(landing, &["commit", "-q", "-m", "balls: found"], None)?;
    Ok(())
}

/// Ensure the configured `tasks_branch` `name` IS the store checkout at `store`
/// — the lazy "a branch is a disk path" primitive `prime` drives between its
/// phases (bl-0a23). Two invariants, each established only when missing, so a
/// re-prime converges to a no-op:
/// - the branch ref `name` exists — a prior clone, or the remote branch the
///   `prime/pre` tracker just fetched into a local ref (clone-in, §12); absent
///   (no remote, or the remote had no such branch — the genuine bootstrap)
///   ⇒ FOUND a fresh orphan root with a tracked `tasks/.gitkeep`;
/// - the store checkout sits on `name` — absent ⇒ add the worktree; present on
///   a DIFFERENT branch (the §12 predicate is "the CONFIGURED branch is the
///   current checkout", not "a store dir exists" — a repointed `tasks_branch`
///   on a once-primed checkout, bl-eb52) ⇒ SWITCH it onto `name`.
///
/// Keyed on `name` (the configured `tasks_branch`) — and `prime/pre` may not
/// move that name: a moved dial aborts the op (bl-698d), so one materialize
/// per prime is the whole story.
pub fn materialize(landing: &Path, store: &Path, name: &str) -> io::Result<()> {
    if !branch_exists(landing, name) {
        found_branch(landing, name)?;
    }
    if !store.exists() {
        git::run(landing, &["worktree", "add", "-q", &store.to_string_lossy(), name], None)?;
    } else if checked_out(store)? != name {
        git::run(store, &["switch", "-q", name], None)?;
    }
    Ok(())
}

/// The branch the `store` checkout currently has — the datum convergence is
/// keyed on (bl-eb52), read from the checkout itself.
fn checked_out(store: &Path) -> io::Result<String> {
    let branch = git::run(store, &["rev-parse", "--abbrev-ref", "HEAD"], None)?;
    Ok(branch.trim().to_string())
}

/// Does `landing` carry a local branch ref named `name`? `show-ref --verify
/// --quiet` exits zero iff the ref resolves — the adopt-vs-found signal, read
/// from LOCAL refs only (core touches no remote, §0): an established branch is
/// either a prior clone or one the tracker's clone-in just created.
fn branch_exists(landing: &Path, name: &str) -> bool {
    git::run(landing, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{name}")], None).is_ok()
}

/// Found a fresh orphan store branch `name` (§2): no ref anywhere offered this
/// history, so this clone bootstraps it. Plumbing builds an orphan root (no
/// parent — the two single-job branches stay independent) carrying a tracked
/// `tasks/.gitkeep`, which keeps `tasks/` present on every checkout (empty dirs
/// are untracked) — one commit, no working-tree round-trip. The REF only:
/// putting it on disk is [`materialize`]'s second invariant.
fn found_branch(landing: &Path, name: &str) -> io::Result<()> {
    let blob = git::run(landing, &["hash-object", "-w", "--stdin"], Some(""))?.trim().to_string();
    let subtree = git::run(landing, &["mktree"], Some(&format!("100644 blob {blob}\t.gitkeep\n")))?.trim().to_string();
    let tree = git::run(landing, &["mktree"], Some(&format!("040000 tree {subtree}\ttasks\n")))?.trim().to_string();
    let root = git::run(landing, &["commit-tree", &tree, "-m", "balls: found store"], None)?.trim().to_string();
    git::run(landing, &["branch", name, &root], None)?;
    Ok(())
}

/// Pin a deterministic commit identity on the new repo so the founding commits
/// (and every later seal here) work headlessly, independent of global git
/// config. Authorship of a ball rides the §5 trailers, not this identity. Set on
/// the landing repo; its linked store worktree inherits the same config.
fn identify(landing: &Path) -> io::Result<()> {
    git::run(landing, &["config", "user.name", "balls"], None)?;
    git::run(landing, &["config", "user.email", "balls@localhost"], None)?;
    Ok(())
}

/// Found a COMPLETE bootstrapped substrate in one call — the landing plus an
/// orphan-founded default store — for callers and tests that want the whole shape
/// eager founding used to make, with no remote in play (bl-0a23).
#[cfg(test)]
pub fn found(landing: &Path, store: &Path, xdg: &Xdg, exe_dir: Option<&Path>) -> io::Result<()> {
    found_landing(landing, xdg, exe_dir)?;
    materialize(landing, store, crate::DEFAULT_TASKS_BRANCH)
}

#[cfg(test)]
#[path = "substrate_tests.rs"]
mod tests;
