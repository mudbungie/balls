//! §12 substrate — `prime`'s bootstrap-on-miss, the retired `init`.
//!
//! Founding is not a separate verb: it is the local-miss branch of idempotent
//! `prime` (§12). [`found`] makes BOTH branches of the two-branch substrate (§2):
//! the **landing** (`balls/config`, holding `config/`) as the repo's first
//! worktree, then the **store** (`tasks_branch`, holding `tasks/`) as a linked
//! orphan-rooted worktree of the same repo. One repo, two branches, two real
//! checkouts — no `operating/` symlink, no terminus to resolve (§1). Core knows
//! nothing of remotes here (§0); it only ensures the two checkouts exist, then
//! `prime` runs the configured chain whose `tracker` handler does the remote work
//! (adopt/found/stealth-lock, §12). Re-running `prime` skips this entirely (the
//! checkout is already a landing), so the whole verb converges to a no-op — there
//! is no `--reinit`.

use crate::git;
use crate::registry::Registry;
use crate::verb::{OpClass, Verb};
use crate::{DEFAULT_TASKS_BRANCH, LANDING_BRANCH};
use std::fs;
use std::io;
use std::path::Path;

/// Run order for the tracker in `sync/prime` `pre` (it imports remote state
/// before reactors run) and in every deliverable verb's `post` — high, so the
/// irreversible push sorts LAST among reactors (§8).
const PRE_ORDER: u32 = 50;
const POST_ORDER: u32 = 90;

/// Found the two-branch substrate (§2 bootstrap-on-miss): the `balls/config`
/// landing at `landing` (seeded `config/` + default tracker wiring, only when
/// `tracker_bin` names an installed binary — else a tracker-free, stealth box)
/// and the `balls/tasks` store at `store` (a linked worktree on an orphan
/// branch, seeded `tasks/`). The caller guarantees neither checkout already
/// exists, so this never clobbers an established checkout.
pub fn found(landing: &Path, store: &Path, tracker_bin: Option<&Path>) -> io::Result<()> {
    fs::create_dir_all(landing)?;
    git::run(landing, &["init", "-q", "-b", LANDING_BRANCH], None)?;
    identify(landing)?;
    seed_config(landing)?;
    if let Some(bin) = tracker_bin {
        wire_tracker(landing)?;
        Registry::at(landing).bind("tracker", bin)?;
    }
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

/// Seed the committed landing substrate: a `.gitignore` for the machine-local
/// `config/plugins/bin/` (§2) and a `config/balls.toml` (§4 — read from the
/// landing, never layered down a trail). Empty `op/phase/` dirs are never seeded
/// — empty means "run nothing" (§12).
fn seed_config(landing: &Path) -> io::Result<()> {
    fs::write(landing.join(".gitignore"), "/config/plugins/bin/\n")?;
    let config = landing.join("config");
    fs::create_dir_all(&config)?;
    fs::write(
        config.join("balls.toml"),
        "# balls config (§4) — read from the landing\n",
    )?;
    Ok(())
}

/// Lay the committed registry symlinks for the default tracker (§6/§12): import
/// in `sync/pre` + `prime/pre`, publish in every deliverable verb's `post`. The
/// LOCAL `bin/tracker` binding is laid separately by [`found`] (it is
/// gitignored — only the portable relative wiring travels with the branch, §2).
fn wire_tracker(landing: &Path) -> io::Result<()> {
    let reg = Registry::at(landing);
    reg.link("sync", "pre", PRE_ORDER, "tracker")?;
    reg.link("prime", "pre", PRE_ORDER, "tracker")?;
    for verb in Verb::ALL.into_iter().filter(|v| v.class() == OpClass::Mutating) {
        reg.link(verb.token(), "post", POST_ORDER, "tracker")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "substrate_tests.rs"]
mod tests;
