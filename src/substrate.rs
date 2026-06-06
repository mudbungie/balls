//! §12 substrate — `prime`'s bootstrap-on-miss, the retired `init`.
//!
//! Founding is not a separate verb: it is the local-miss branch of idempotent
//! `prime` (§12). [`found`] makes the **landing** — `git init` an orphan `balls`
//! branch, seed `config/` + the `tasks/` store, lay the default tracker wiring,
//! and commit once. Core knows nothing of remotes here (§0); it only ensures the
//! landing exists, then `prime` runs the configured chain whose `tracker`
//! handler does the remote work (adopt/found/stealth-lock, §12). Re-running
//! `prime` skips this entirely (the checkout is already a landing), so the whole
//! verb converges to a no-op — there is no `--reinit`.

use crate::git;
use crate::registry::Registry;
use crate::verb::{OpClass, Verb};
use crate::STATE_BRANCH;
use std::fs;
use std::io;
use std::path::Path;

/// Run order for the tracker in `sync/prime` `pre` (it imports remote state
/// before reactors run) and in every deliverable verb's `post` — high, so the
/// irreversible push sorts LAST among reactors (§8).
const PRE_ORDER: u32 = 50;
const POST_ORDER: u32 = 90;

/// Found the landing at `operating` (§12 bootstrap-on-miss): an orphan `balls`
/// branch with seeded `config/` + `tasks/`, the default tracker wiring (only
/// when `tracker_bin` names an installed binary — else a tracker-free, stealth
/// box), and one `balls: found` commit. The caller guarantees `operating` is not
/// already a landing, so this never clobbers an established checkout.
pub fn found(operating: &Path, tracker_bin: Option<&Path>) -> io::Result<()> {
    fs::create_dir_all(operating)?;
    git::run(operating, &["init", "-q", "-b", STATE_BRANCH], None)?;
    identify(operating)?;
    seed(operating)?;
    if let Some(bin) = tracker_bin {
        wire_tracker(operating)?;
        Registry::at(operating).bind("tracker", bin)?;
    }
    git::run(operating, &["add", "-A"], None)?;
    git::run(operating, &["commit", "-q", "-m", "balls: found"], None)?;
    Ok(())
}

/// Pin a deterministic commit identity on the new repo so the founding commit
/// (and every later seal here) works headlessly, independent of global git
/// config. Authorship of a ball rides the §5 trailers, not this identity.
fn identify(operating: &Path) -> io::Result<()> {
    git::run(operating, &["config", "user.name", "balls"], None)?;
    git::run(operating, &["config", "user.email", "balls@localhost"], None)?;
    Ok(())
}

/// Seed the committed substrate: a `.gitignore` for the machine-local
/// `config/plugins/bin/` (§2), a `config/balls.toml` (§4 values layer down the
/// trail at read time), and the `tasks/` store dir. Empty `op/phase/` dirs are
/// never seeded — empty means "run nothing" (§12).
fn seed(operating: &Path) -> io::Result<()> {
    fs::write(operating.join(".gitignore"), "/config/plugins/bin/\n")?;
    let config = operating.join("config");
    fs::create_dir_all(&config)?;
    fs::write(
        config.join("balls.toml"),
        "# balls config (§4) — fields layer down the trail at read time\n",
    )?;
    fs::create_dir_all(operating.join("tasks"))?;
    Ok(())
}

/// Lay the committed registry symlinks for the default tracker (§6/§12): import
/// in `sync/pre` + `prime/pre`, publish in every deliverable verb's `post`. The
/// LOCAL `bin/tracker` binding is laid separately by [`found`] (it is
/// gitignored — only the portable relative wiring travels with the branch, §2).
fn wire_tracker(operating: &Path) -> io::Result<()> {
    let reg = Registry::at(operating);
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
