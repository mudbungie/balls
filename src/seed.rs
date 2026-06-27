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
//! ([`include_str!`]) and used DIRECTLY as the seed — so a fresh `cargo install`
//! or a test binary run from `/tmp` always carries the CURRENT default (no "run a
//! script to get set up"). `$XDG_CONFIG_HOME/balls/default-config/` is a
//! DELIBERATE, never-auto-written override: present, its files win per-file (an
//! override that omits a file falls back to the embedded default for that file),
//! letting an org/user customize the default capability set without touching core
//! (§1). Core NEVER creates that folder, so a once-materialized copy can't go
//! stale and silently shadow a moved embedded default (bl-8088).
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

/// The embedded install-default `balls.toml` (§4) — the authoritative default a
/// fresh landing seeds from unless an XDG override `balls.toml` is present.
const EMBEDDED_BALLS: &str = include_str!("../default-config/balls.toml");

/// The embedded install-default `plugins.toml` (§6) — the `[hooks]` schedule
/// wiring the shipped `bl-tracker` + `bl-delivery` capabilities, used directly
/// unless an XDG override `plugins.toml` is present.
const EMBEDDED_PLUGINS: &str = include_str!("../default-config/plugins.toml");

/// Seed a fresh landing's `config/` from the default-config (§12). Writes
/// `balls.toml` verbatim, then writes `plugins.toml` with each named plugin bound
/// to its sibling binary beside `bl` and every absent-binary entry PRUNED. Each
/// file's content is the embedded install-default unless an XDG override file is
/// present ([`default_body`]). `exe_dir` is the directory holding `bl` (where the
/// shipped siblings live); `None` ⇒ a tracker-less box (every entry prunes, the
/// chain runs empty).
pub fn seed_landing(xdg: &Xdg, landing: &Path, exe_dir: Option<&Path>) -> io::Result<()> {
    let override_dir = xdg.default_config();
    let config = landing.join("config");
    fs::create_dir_all(&config)?;
    fs::write(config.join("balls.toml"), default_body(&override_dir, "balls.toml", EMBEDDED_BALLS)?)?;

    let mut hooks = Hooks::parse(&default_body(&override_dir, "plugins.toml", EMBEDDED_PLUGINS)?)?;
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

/// The default-config content for `name` (§1): the XDG override file
/// `$XDG_CONFIG_HOME/balls/default-config/<name>` when a user/org has already
/// authored it, else the `embedded` install-default. The override folder is read
/// ONLY when present — never created — so the live embedded default can never be
/// shadowed by a stale once-materialized copy (bl-8088, "don't store what you can
/// compute").
fn default_body(override_dir: &Path, name: &str, embedded: &str) -> io::Result<String> {
    let file = override_dir.join(name);
    if file.is_file() {
        fs::read_to_string(&file)
    } else {
        Ok(embedded.to_string())
    }
}

#[cfg(test)]
#[path = "seed_tests.rs"]
mod tests;
