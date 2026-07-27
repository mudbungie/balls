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
use crate::message::Message;
use crate::seed;
use crate::verb::Verb;
use crate::LANDING_BRANCH;
use std::fs;
use std::io;
use std::path::Path;

/// Is the landing already founded? A founded landing has a COMMIT on the
/// `balls/config` branch (§12) — founding's ONE commit point, not the `config/`
/// folder [`found_landing`] creates on its way there (bl-ffbf).
///
/// The directory this once tested is created BEFORE that commit, so a crash in
/// between left debris a directory test called "founded" forever: every later op
/// then opened its change worktree on an unborn HEAD and failed, with no act
/// that could ever converge it. Keyed on the commit, the same debris is simply
/// "not founded yet" — and founding runs again straight over it (idempotent by
/// construction: `git init`, the seed and the `.gitignore` all overwrite what a
/// crashed founding left), so the general path IS the recovery and there is no
/// bootstrap special case to write.
pub fn is_landing(landing: &Path) -> bool {
    let refname = format!("refs/heads/{LANDING_BRANCH}");
    git::run(landing, &["rev-parse", "--verify", "--quiet", &refname], None).is_ok()
}

/// Found the landing half of the substrate (§2 bootstrap-on-miss): the
/// `balls/config` branch at `landing`, its `config/` SEEDED from the app
/// default-config (the `balls.toml` + the `plugins.toml` hook schedule, with each
/// named plugin found beside `bl` in `exe_dir` bound and every absent-binary entry
/// pruned, §12). Returns the seed's rendered prune notes (a pruned name with a
/// `[source]` hint, bl-5b09) for `prime` to emit through the op log once it has
/// one — founding necessarily precedes the log's threshold read. The caller
/// guarantees [`is_landing`] is false, so this never clobbers an established
/// checkout.
///
/// Founding is a TRANSACTION whose one commit point is the seal at the end
/// (bl-ffbf): every step before it OVERWRITES rather than creates — `git init`
/// re-inits, the `.gitignore` and the seeded `config/` rewrite — so a re-run
/// straight over the debris of a crashed founding converges. That is the
/// ordinary path with the seed already on disk, not a bootstrap special case;
/// there is nothing to detect and nothing to repair. The STORE is NOT founded
/// here — that is
/// [`materialize`]'s lazy job, run after the tracker's `prime/pre` has
/// had its chance to clone an established remote branch in (bl-0a23).
///
/// The ONE piece of crash debris that transaction cannot overwrite is git's own
/// `.git/index.lock` (bl-3e89): `git init` re-inits and the seed rewrites, but the
/// `git add -A` below fails on a leftover lock with git's raw error, and prime's
/// debris report — which runs only AFTER founding — never gets to speak. So the
/// same report line refuses here, up front, before any half-founding work:
/// founding cannot delete the lock (it may be LIVE, and prime never deletes what
/// may hold work), so it names it and the removal in the debris-report voice.
pub fn found_landing(landing: &Path, xdg: &Xdg, exe_dir: Option<&Path>, actor: &str) -> io::Result<Vec<String>> {
    if let Some(note) = crate::converge::index_lock(landing) {
        return Err(io::Error::other(note));
    }
    fs::create_dir_all(landing)?;
    git::run(landing, &["init", "-q", "-b", LANDING_BRANCH], None)?;
    identify(landing)?;
    fs::write(landing.join(".gitignore"), "/config/plugins/bin/\n")?;
    let notes = seed::seed_landing(xdg, landing, exe_dir)?;
    git::run(landing, &["add", "-A"], None)?;
    let message = Message::checkout(Verb::Prime, actor, "balls: found".into()).render()?;
    git::run(landing, &["commit", "-q", "-F", "-"], Some(&message))?;
    Ok(notes)
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
pub fn materialize(landing: &Path, store: &Path, name: &str, actor: &str) -> io::Result<()> {
    if !branch_exists(landing, name) {
        found_branch(landing, name, actor)?;
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
fn found_branch(landing: &Path, name: &str, actor: &str) -> io::Result<()> {
    let blob = git::run(landing, &["hash-object", "-w", "--stdin"], Some(""))?.trim().to_string();
    let subtree = git::run(landing, &["mktree"], Some(&format!("100644 blob {blob}\t.gitkeep\n")))?.trim().to_string();
    let tree = git::run(landing, &["mktree"], Some(&format!("040000 tree {subtree}\ttasks\n")))?.trim().to_string();
    let message = Message::checkout(Verb::Prime, actor, "balls: found store".into()).render()?;
    let root = git::run(landing, &["commit-tree", &tree], Some(&message))?.trim().to_string();
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

/// The bl-b915 founding advisory: report-only scrutiny, zero mechanism added to
/// resolution — never a refusal, never a redirect. Call only on the miss branch
/// of `prime`, right before [`found_landing`]: the caller is ABOUT to found a
/// brand-new store at `invocation_path`, so if a founded store already sits at
/// some ANCESTOR directory (balls' own record — [`Xdg::nearest_founded_ancestor`]
/// stats the ancestor's own clone dir, git never consulted), that is almost
/// always the bl-0bd8 invisible-sibling-substrate footgun rather than a
/// deliberate nested/sibling store: warn on stderr, naming the `-C` escape
/// hatch (bl-c620), and let founding proceed regardless.
pub fn warn_founded_ancestor(xdg: &Xdg, invocation_path: &Path) {
    if let Some(ancestor) = xdg.nearest_founded_ancestor(invocation_path) {
        let a = ancestor.display();
        eprintln!("prime: founding a new store here; an existing store sits at {a} — meant that one? (cd there, or bl -C {a})");
    }
}

/// Found a COMPLETE bootstrapped substrate in one call — the landing plus an
/// orphan-founded default store — for callers and tests that want the whole shape
/// eager founding used to make, with no remote in play (bl-0a23). Founds as the
/// fixture actor `tester` (the test edges' `default_actor`).
#[cfg(test)]
pub fn found(landing: &Path, store: &Path, xdg: &Xdg, exe_dir: Option<&Path>) -> io::Result<()> {
    found_landing(landing, xdg, exe_dir, "tester")?; // seed notes dropped: fixtures carry no op log
    materialize(landing, store, crate::DEFAULT_TASKS_BRANCH, "tester")
}

#[cfg(test)]
#[path = "substrate_tests.rs"]
mod tests;
