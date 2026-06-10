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
use std::fs;
use std::io;
use std::path::Path;
use toml::value::{Table, Value};

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

/// The per-machine store remote named in the XDG user config's `remote` key — the
/// §12 precedence layer between an explicit CLI override and auto-discovered
/// `origin` (`--remote` > `--center` > XDG > `origin`). The remote is NOT a
/// landing field: it never travels on `install` (a remote URL is per-machine, not
/// shared config, §4), so it lives only in this per-machine layer or is discovered
/// from `origin`. An absent file/key ⇒ `None`; a malformed file ⇒ `None` too — the
/// same file is read by [`EffectiveConfig::resolve`], which surfaces the parse
/// error, so this stays quiet rather than double-reporting.
pub fn xdg_remote(user_config: &Path) -> Option<String> {
    let table = read_layer(user_config).ok().flatten()?;
    table.get("remote")?.as_str().map(str::to_string)
}

/// Read one `config/balls.toml` layer. Absent ⇒ `None` (the layer is empty, the
/// common un-configured case); malformed ⇒ an error naming the file; any other
/// read error propagates. Shared with [`crate::conf`], whose provenance read
/// inspects each layer table individually (§4).
pub(crate) fn read_layer(path: &Path) -> io::Result<Option<Table>> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str::<Table>(&text)
            .map(Some)
            .map_err(|e| io::Error::other(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Apply §4 merge of `inner` over `base`, `inner` winning. A `<field>_prepend`/
/// `_append`/`_ban` key composes the list at `<field>`; every other key (scalar
/// or object) fully replaces its `base` entry. Shared with [`crate::hooks`], which
/// layers the `[hooks]` lists the SAME way (§4/§6, bl-8540) — one merge, not two.
pub(crate) fn layer_over(base: &mut Table, inner: Table) {
    for (key, value) in inner {
        match list_directive(&key) {
            Some((field, op)) => compose_list(base, field, op, value),
            None => {
                base.insert(key, value);
            }
        }
    }
}

/// The list-compose directives (§4). A bare `<field>` is plain replacement and
/// is NOT a directive — only these three suffixes compose.
#[derive(Clone, Copy)]
enum ListOp {
    Prepend,
    Append,
    Ban,
}

/// Split a `<field><suffix>` key into its target field and compose op, or `None`
/// for a plain key. A bare suffix with no field (`_append`) is not a directive.
fn list_directive(key: &str) -> Option<(&str, ListOp)> {
    const SUFFIXES: [(&str, ListOp); 3] = [
        ("_prepend", ListOp::Prepend),
        ("_append", ListOp::Append),
        ("_ban", ListOp::Ban),
    ];
    SUFFIXES.into_iter().find_map(|(suffix, op)| {
        key.strip_suffix(suffix)
            .filter(|field| !field.is_empty())
            .map(|field| (field, op))
    })
}

/// Compose `value` (treated as a list) into the list at `base[field]` per `op`:
/// prepend/append concatenate; ban removes every element `value` contains. A
/// non-list current value (or incoming) is treated as empty — the directive
/// then seeds the field, and a type clash surfaces at projection (§4).
fn compose_list(base: &mut Table, field: &str, op: ListOp, value: Value) {
    let incoming = as_array(&value);
    let current = base.get(field).map(as_array).unwrap_or_default();
    let merged: Vec<Value> = match op {
        ListOp::Prepend => incoming.into_iter().chain(current).collect(),
        ListOp::Append => current.into_iter().chain(incoming).collect(),
        ListOp::Ban => current.into_iter().filter(|v| !incoming.contains(v)).collect(),
    };
    base.insert(field.to_string(), Value::Array(merged));
}

/// A TOML value's elements if it is an array, else empty — the lenient read the
/// list directives share (a clash becomes a projection error, not a panic).
fn as_array(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
