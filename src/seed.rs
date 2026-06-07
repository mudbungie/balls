//! §1/§12 the seed — the app default-config, copied into a fresh landing.
//!
//! The trusted default capability set is config-time, not run-time: there is NO
//! runtime plugin magic, core only ever runs what the landing's `plugins.toml`
//! lists (§0). So the defaults live in a real folder a fresh `bl prime` copies
//! in — making the tracker + delivery plugins ordinary landing entries and the
//! default set swappable POLICY (an org ships its own seed) rather than core code
//! (severability, §1).
//!
//! **Bootstrap source.** The default-config is EMBEDDED in the binary
//! ([`include_str!`]) and written out to `$XDG_CONFIG_HOME/balls/default-config/`
//! on first prime when that folder is ABSENT — so a fresh `cargo install` or a
//! test binary run from `/tmp` always has a seed (no "run a script to get set
//! up"). The XDG folder is an OPTIONAL override: present, it wins, letting an
//! org/user customize the default capability set without touching core (§1).
//!
//! **Bind + prune.** Seeding binds each plugin the schedule names to its sibling
//! binary beside `bl` ([`Registry::bind`]) and PRUNES the hook entries whose
//! binary is absent here, so a tracker-less or test box never aborts (§12).
//! [`rebind`] re-establishes those local `bin/<name>` symlinks on an established
//! landing (a new machine / clone re-deriving the gitignored half), without
//! re-seeding or pruning the committed schedule.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::hooks::Hooks;
use crate::layout::Xdg;
use crate::registry::Registry;

/// The embedded install-default `balls.toml` (§4) — the seed of last resort, used
/// to populate `$XDG_CONFIG_HOME/balls/default-config/` on first prime.
const EMBEDDED_BALLS: &str = include_str!("../default-config/balls.toml");

/// The embedded install-default `plugins.toml` (§6) — the `[hooks]` schedule
/// wiring the shipped `tracker` + `bl-delivery` capabilities.
const EMBEDDED_PLUGINS: &str = include_str!("../default-config/plugins.toml");

/// Seed a fresh landing's `config/` from the default-config source (§12). Copies
/// `balls.toml` verbatim, then writes `plugins.toml` with each named plugin bound
/// to its sibling binary beside `bl` and every absent-binary entry PRUNED.
/// `exe_dir` is the directory holding `bl` (where the shipped siblings live);
/// `None` ⇒ a tracker-less box (every entry prunes, the chain runs empty).
pub fn seed_landing(xdg: &Xdg, landing: &Path, exe_dir: Option<&Path>) -> io::Result<()> {
    let source = ensure_default_config(xdg)?;
    let config = landing.join("config");
    fs::create_dir_all(&config)?;
    copy_if_present(&source.join("balls.toml"), &config.join("balls.toml"))?;

    let mut hooks = Hooks::load_from(&source.join("plugins.toml"))?;
    let present = bind_present(landing, exe_dir, &hooks)?;
    hooks.retain(|name| present.contains(name));
    fs::write(config.join("plugins.toml"), hooks.to_toml())?;
    Ok(())
}

/// Re-establish the local `bin/<name>` bindings for an established landing's
/// committed schedule (§12) — the gitignored half a new machine / clone must
/// re-derive. Idempotent; never prunes or rewrites the committed `plugins.toml`
/// (capabilities change only by `bl install`, never a re-prime).
pub fn rebind(landing: &Path, exe_dir: Option<&Path>) -> io::Result<()> {
    let hooks = Hooks::load(landing)?;
    bind_present(landing, exe_dir, &hooks)?;
    Ok(())
}

/// Bind every plugin the `hooks` schedule names to its sibling binary beside
/// `bl`, when present, returning the set that resolved. Shared by [`seed_landing`]
/// (which prunes the rest) and [`rebind`] (which leaves the schedule untouched).
fn bind_present(landing: &Path, exe_dir: Option<&Path>, hooks: &Hooks) -> io::Result<BTreeSet<String>> {
    let registry = Registry::at(landing);
    let mut present = BTreeSet::new();
    for name in hooks.referenced().keys() {
        if let Some(bin) = sibling(exe_dir, name) {
            registry.bind(name, &bin)?;
            present.insert(name.clone());
        }
    }
    Ok(present)
}

/// The path to a `name`d binary beside `bl` (in `exe_dir`), if it exists — how a
/// shipped sibling plugin is found (§6/§12). An absent `exe_dir` or missing
/// sibling ⇒ `None` (that plugin prunes from the seed; stays dangling on rebind).
fn sibling(exe_dir: Option<&Path>, name: &str) -> Option<PathBuf> {
    let path = exe_dir?.join(name);
    path.exists().then_some(path)
}

/// Resolve the default-config source folder (§1): the XDG override
/// `$XDG_CONFIG_HOME/balls/default-config/` if it exists, else materialize the
/// embedded default there on first prime and use that. Either way the return is a
/// real on-disk folder [`seed_landing`] reads the two files from.
fn ensure_default_config(xdg: &Xdg) -> io::Result<PathBuf> {
    let dir = xdg.default_config();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("balls.toml"), EMBEDDED_BALLS)?;
        fs::write(dir.join("plugins.toml"), EMBEDDED_PLUGINS)?;
    }
    Ok(dir)
}

/// Copy `src` → `dest` when `src` exists; an absent source contributes nothing
/// (an override folder may omit a file — that field then falls to its default).
fn copy_if_present(src: &Path, dest: &Path) -> io::Result<()> {
    if src.is_file() {
        fs::copy(src, dest)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "seed_tests.rs"]
mod tests;
