//! §12 substrate — `prime`'s bootstrap-on-miss, the retired `init`.
//!
//! Founding is not a separate verb: it is the local-miss branch of idempotent
//! `prime` (§12). [`found`] makes BOTH branches of the two-branch substrate (§2):
//! the **landing** (`balls/config`, holding `config/`) as the repo's first
//! worktree, then the **store** (`tasks_branch`, holding `tasks/`) as a linked
//! orphan-rooted worktree of the same repo. One repo, two branches, two real
//! checkouts — no symlink indirection, no chain to resolve (§1). Core knows
//! nothing of remotes here (§0); it only ensures the two checkouts exist and
//! seeds the landing's `config/` from the app default-config ([`crate::seed`]),
//! then `prime` runs the configured chain whose `tracker` handler does the remote
//! work (adopt/found/stealth-lock, §12). Re-running `prime` skips this entirely
//! (the checkout is already a landing), so the whole verb converges to a no-op —
//! there is no `--reinit`.

use crate::git;
use crate::layout::Xdg;
use crate::seed;
use crate::{DEFAULT_TASKS_BRANCH, LANDING_BRANCH};
use std::fs;
use std::io;
use std::path::Path;

/// Found the two-branch substrate (§2 bootstrap-on-miss): the `balls/config`
/// landing at `landing` (its `config/` SEEDED from the app default-config — the
/// `balls.toml` + the `plugins.toml` hook schedule, with each named plugin found
/// beside `bl` in `exe_dir` bound and every absent-binary entry pruned, §12) and
/// the `balls/tasks` store at `store` (a linked worktree on an orphan branch,
/// seeded `tasks/`). The caller guarantees neither checkout already exists, so
/// this never clobbers an established checkout.
pub fn found(landing: &Path, store: &Path, xdg: &Xdg, exe_dir: Option<&Path>) -> io::Result<()> {
    fs::create_dir_all(landing)?;
    git::run(landing, &["init", "-q", "-b", LANDING_BRANCH], None)?;
    identify(landing)?;
    fs::write(landing.join(".gitignore"), "/config/plugins/bin/\n")?;
    seed::seed_landing(xdg, landing, exe_dir)?;
    git::run(landing, &["add", "-A"], None)?;
    git::run(landing, &["commit", "-q", "-m", "balls: found"], None)?;
    found_store(landing, store)?;
    Ok(())
}

/// Lay the STORE as a linked worktree of the landing repo (§2): an orphan-rooted
/// `tasks_branch` (no shared history with the landing — the two single-job
/// branches), checked out at `store` with a seeded `tasks/` folder. Plumbing
/// builds the root with no parent (so the two branches stay independent) carrying
/// a tracked `tasks/.gitkeep`, which keeps `tasks/` present on every checkout
/// (empty dirs are untracked) — one commit, no working-tree round-trip.
fn found_store(landing: &Path, store: &Path) -> io::Result<()> {
    let blob = git::run(landing, &["hash-object", "-w", "--stdin"], Some(""))?.trim().to_string();
    let subtree = git::run(landing, &["mktree"], Some(&format!("100644 blob {blob}\t.gitkeep\n")))?.trim().to_string();
    let tree = git::run(landing, &["mktree"], Some(&format!("040000 tree {subtree}\ttasks\n")))?.trim().to_string();
    let root = git::run(landing, &["commit-tree", &tree, "-m", "balls: found store"], None)?.trim().to_string();
    git::run(landing, &["branch", DEFAULT_TASKS_BRANCH, &root], None)?;
    git::run(landing, &["worktree", "add", "-q", &store.to_string_lossy(), DEFAULT_TASKS_BRANCH], None)?;
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

#[cfg(test)]
#[path = "substrate_tests.rs"]
mod tests;
