//! §4 config VALUES — the `EffectiveConfig`, read from the LANDING.
//!
//! Config's durable home is the landing (`balls/config`); it is NEVER read from
//! the store and NEVER layered down a trail (there is no trail — §12). The
//! EFFECTIVE config is the landing's `config/balls.toml` overlaid by the
//! per-machine XDG user file, with built-in serde defaults beneath. A center's
//! config reaches you only by `install` copying it INTO the landing (§6), where
//! it becomes local — so this read is the sole authority for what runs.
//!
//! [`EffectiveConfig::resolve`] is PURE over LOCAL checkouts: the caller hands in
//! the landing checkout and the XDG user-config path; this reads each
//! `config/balls.toml` and folds them per §4. It never fetches.
//!
//! §4 layers, INNERMOST wins (highest priority first):
//!   1. CLI flags                                   — a documented seam (below)
//!   2. `$XDG_CONFIG_HOME/balls/config.toml`        — `user_config`
//!   3. `config/balls.toml` on the landing
//!   4. built-in defaults                           — serde fills any absent field
//!
//! Merge semantics (§4): scalar/object fields — innermost layer fully replaces
//! outer (objects are NOT deep-merged). List fields — bare `<field>` = full
//! replacement; compose with `<field>_prepend` / `<field>_append` / `<field>_ban`.
//!
//! The §4 layer-1 CLI override is an unbuilt seam: no flag consumes `tasks_branch`
//! today, so wiring an argv layer here would be a consumer-less mechanism. When
//! a flag needs it, it composes as one more (highest) table.

use crate::DEFAULT_TASKS_BRANCH;
use serde::Deserialize;
use std::io;
use std::path::Path;
use toml::value::{Table, Value};

// The §4 TOML layer-merge primitives (a general utility shared with
// [`crate::hooks`]) live in a sibling; re-exported so consumers keep reaching
// `crate::config::{read_layer, layer_over}`.
#[path = "config_merge.rs"]
mod merge;
pub(crate) use merge::{layer_over, read_layer};

/// The resolved §4 config — the built-in fields balls core reads. Other keys in
/// `config/balls.toml` are layered through the merge but ignored on projection
/// (serde drops unknown keys), so a team/plugin key round-trips through the fold
/// without core having to know it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EffectiveConfig {
    /// The STORE branch the `tasks/` checkout rides (§2/§4), default
    /// [`DEFAULT_TASKS_BRANCH`] — the one config→store indirection (§4). The
    /// landing branch is path-derived and never named here (you read config FROM
    /// it, so it cannot name where it lives).
    #[serde(default = "default_tasks_branch")]
    pub tasks_branch: String,

    /// The §4 threshold for the unified op log (§1/§6), default `"info"` — a plain
    /// serde-default scalar like `tasks_branch`. A run-time `--log-level` is the
    /// layer-1 CLI override (it reads as [`crate::log::Level`]); this is the
    /// persistent layers-2/3 value beneath it.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_tasks_branch() -> String {
    DEFAULT_TASKS_BRANCH.to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for EffectiveConfig {
    fn default() -> EffectiveConfig {
        EffectiveConfig { tasks_branch: default_tasks_branch(), log_level: default_log_level() }
    }
}

impl EffectiveConfig {
    /// Resolve the §4 config from the LANDING. Reads the landing's
    /// `config/balls.toml` and the XDG `user_config` (supplied by the edge — no
    /// env reads here, the bl-bfa8 rule), folding them so the user config
    /// (layer 2) wins over the landing (layer 3); built-in defaults are the
    /// implicit base (serde fills any field no layer set). There is no trail —
    /// config lives on the landing alone (§12).
    ///
    /// An absent layer file contributes nothing; a malformed one is an error
    /// naming the file. The merged table is projected onto the typed fields.
    pub fn resolve(landing: &Path, user_config: &Path) -> io::Result<EffectiveConfig> {
        let mut merged = Table::new();
        if let Some(layer) = read_layer(&landing.join("config").join("balls.toml"))? {
            layer_over(&mut merged, layer);
        }
        if let Some(layer) = read_layer(user_config)? {
            layer_over(&mut merged, layer);
        }
        let cfg: EffectiveConfig = Value::Table(merged)
            .try_into()
            .map_err(|e| io::Error::other(format!("config does not resolve: {e}")))?;
        forbid_landing(&cfg.tasks_branch)?;
        Ok(cfg)
    }
}

/// Refuse a `tasks_branch` that names the LANDING branch (§2/§4, bl-ac89). The
/// coincident name is structurally impossible — `config/` and `tasks/` are two
/// worktrees of ONE local repo, and git refuses a branch checked out twice — and
/// §4 independently forbids what it would mean: the landing is single-owner,
/// never pushed, never sync-merged, so it cannot double as the store. ONE
/// invariant, two doors: the read authority ([`EffectiveConfig::resolve`] — a
/// seeded, adopted, or hand-edited poison fails NAMED on every op instead of
/// wedging prime on a raw git fatal) and the `conf set task-branch` write
/// ([`crate::conf`], the log-level ladder-validation precedent).
pub(crate) fn forbid_landing(tasks_branch: &str) -> io::Result<()> {
    if tasks_branch == crate::LANDING_BRANCH {
        return Err(io::Error::other(format!(
            "tasks_branch '{tasks_branch}' names the landing — one branch cannot back two checkouts, \
             and the landing is single-owner, never a store (§2/§4); pick another: bl conf set task-branch <branch>"
        )));
    }
    Ok(())
}

/// The §12 stealth sentinel — the one value the landing `task_remote` rung may
/// hold: "the store's remote is nothing, on purpose". Stealth is not a mode or
/// a lock file; it is federation's zero case, a value on the ONE remote ladder
/// (bl-9df0).
pub const STEALTH_REMOTE: &str = "none";

/// Read a `remote` URL key from a §12 store-remote TOML layer — the XDG user
/// config or a clone's `binding.toml`, read identically. An absent file/key ⇒
/// `None`; a malformed file ⇒ `None` too — a remote URL never travels on
/// `install` (§4), so its only homes are these layers or a discovered `origin`.
fn remote_key(path: &Path) -> Option<String> {
    let table = read_layer(path).ok().flatten()?;
    table.get("remote")?.as_str().map(str::to_string)
}

/// The per-clone store remote named in this checkout's `binding.toml` `remote`
/// key — the §12 DURABLE tier between the landing stealth sentinel and the legacy
/// XDG remote (bl-d081). The store remote is a PER-CHECKOUT fact (which center
/// THIS clone tracks), so its authoritative home is the per-clone binding — local
/// state that never travels on `install` and can never shadow another repo's
/// store, the machine-wide-XDG footgun this layer replaces. Absent/malformed ⇒
/// `None`; `bl conf set task-remote <url>` writes it ([`crate::conf`]).
pub fn binding_remote(binding: &Path) -> Option<String> {
    remote_key(binding)
}

/// The per-machine store remote named in the XDG user config's `remote` key — the
/// §12 LEGACY tier beneath the per-clone binding (bl-d081). Kept READ-ONLY for
/// back-compat: a machine that wrote a global `remote` before the per-clone home
/// still resolves it, but new writes land per-clone, so one repo's setup can no
/// longer redirect every other repo's store. Absent file/key ⇒ `None`; a
/// malformed file ⇒ `None` too — the same file is read by
/// [`EffectiveConfig::resolve`], which surfaces the parse error, so this stays
/// quiet rather than double-reporting.
pub fn xdg_remote(user_config: &Path) -> Option<String> {
    remote_key(user_config)
}

/// The per-checkout store-remote POLICY — the landing `balls.toml` `task_remote`
/// key, the §12 rung between the per-op flag and the per-machine XDG remote.
/// Today it legally holds only [`STEALTH_REMOTE`] ("declared stealth"): remote
/// URLs stay per-machine (§4), but the stealth policy is the CHECKOUT's, lives
/// in its config, and travels on `install` like any other team policy. A raw
/// read — [`remote_ladder`] enforces the legal value.
pub fn landing_remote(landing: &Path) -> io::Result<Option<String>> {
    let layer = read_layer(&landing.join("config").join("balls.toml"))?;
    Ok(layer.and_then(|t| t.get("task_remote").and_then(Value::as_str).map(str::to_string)))
}

/// Resolve the EXPLICIT tiers of the ONE §12 remote ladder — per-op
/// `--remote`/`--center` > landing `task_remote` > per-clone `binding.toml`
/// remote > legacy XDG `remote` — returning the explicit remote and whether
/// stealth is DECLARED. Consent given supersedes consent withheld: a per-op
/// remote outranks the sentinel for that one op. A declared sentinel STOPS
/// resolution — no binding/XDG fallback, and the stealth bit rides the binding so
/// the tracker skips even its implicit `origin` discovery beneath (the §12 "locks
/// the store local" promise, now derived per op from config instead of written
/// once to a lock file — bl-9df0). The per-clone binding outranks the legacy
/// per-machine XDG remote (bl-d081): a remote is a per-checkout fact, so the
/// per-clone home is more specific; XDG remains only a read-only back-compat
/// fallback. A landing value other than the sentinel is refused: a URL's durable
/// home is the per-clone binding, not the shared landing config.
pub fn remote_ladder(cli: Option<String>, landing: &Path, binding: &Path, user_config: &Path) -> io::Result<(Option<String>, bool)> {
    if cli.is_some() {
        return Ok((cli, false));
    }
    match landing_remote(landing)? {
        Some(v) if v == STEALTH_REMOTE => Ok((None, true)),
        Some(v) => Err(io::Error::other(format!(
            "task_remote '{v}' in the landing config — the landing holds only the stealth sentinel \
             '{STEALTH_REMOTE}' (a remote URL's home is this clone's binding: `bl conf set task-remote <url>`, §4/§12)"
        ))),
        None => Ok((binding_remote(binding).or_else(|| xdg_remote(user_config)), false)),
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
