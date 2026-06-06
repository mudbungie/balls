//! §4 config VALUES — the layered `EffectiveConfig`, resolved down the trail.
//!
//! Config VALUES (`config/balls.toml` scalars/objects/lists) layer DOWN the
//! §12 trail at READ time, innermost(landing)-wins, no depth cap. This is the
//! declarative half of the trail asymmetry (§12): config VALUES auto-layer
//! because they are data, not code — "shadow config, not merge task lists." The
//! executable plugin chain does NOT layer (that is `bl install`'s consented
//! job), and `tasks/` does NOT federate (exactly one store, at the terminus).
//!
//! [`EffectiveConfig::resolve`] is PURE over LOCAL checkouts: the caller hands
//! in the ordered trail ([`crate::trail::walk`] output, landing-first) and the
//! XDG user-config path; this reads each `config/balls.toml` and folds them per
//! §4. Materializing remote hops into the local trail is the tracker's job
//! (§12 SEAM) — this never fetches, so stealth (trail length 1) and a federated
//! trail run the identical code.
//!
//! §4 layers, INNERMOST wins (highest priority first):
//!   1. CLI flags                                   — a documented seam (below)
//!   2. `$XDG_CONFIG_HOME/balls/config.toml`        — `user_config`
//!   3. `config/balls.toml` on this checkout's landing
//!   4. `config/balls.toml` on each downstream trail step (terminus is outermost)
//!   5. built-in defaults                           — serde fills any absent field
//!
//! Merge semantics (§4): scalar/object fields — innermost layer fully replaces
//! outer (objects are NOT deep-merged). List fields — bare `<field>` = full
//! replacement; compose with `<field>_prepend` / `<field>_append` / `<field>_ban`.
//!
//! The §4 layer-1 CLI override is an unbuilt seam: no flag consumes `branch`/
//! `id_scheme` today, so wiring an argv layer here would be a consumer-less
//! mechanism. When a flag needs it, it composes as one more (highest) table.

use crate::id::IdScheme;
use crate::STATE_BRANCH;
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
    /// The branch every checkout roots its task store + config on (§2/§4),
    /// default [`STATE_BRANCH`] — the one config-overridable bootstrap fact.
    #[serde(default = "default_branch")]
    pub branch: String,
    /// How `create` mints fresh ids (§ id generation), default [`IdScheme`].
    #[serde(default)]
    pub id_scheme: IdScheme,
}

fn default_branch() -> String {
    STATE_BRANCH.to_string()
}

impl Default for EffectiveConfig {
    fn default() -> EffectiveConfig {
        EffectiveConfig {
            branch: default_branch(),
            id_scheme: IdScheme::default(),
        }
    }
}

impl EffectiveConfig {
    /// Resolve the §4 layered config. `trail` is the §12 walk output
    /// (landing-first); `user_config` is the XDG `config.toml` path. Reads every
    /// `config/balls.toml` and folds them OUTERMOST-first so each higher layer
    /// wins: trail terminus→landing, then the user config. Built-in defaults are
    /// the implicit base (serde fills any field no layer set).
    ///
    /// An absent layer file contributes nothing; a malformed one is an error
    /// naming the file. The merged table is projected onto the typed fields.
    pub fn resolve(trail: &[std::path::PathBuf], user_config: &Path) -> io::Result<EffectiveConfig> {
        let mut merged = Table::new();
        // Apply the trail terminus→landing (reverse of landing-first) so the
        // landing — the innermost trail layer — wins (§4/§12).
        for checkout in trail.iter().rev() {
            if let Some(layer) = read_layer(&checkout.join("config").join("balls.toml"))? {
                layer_over(&mut merged, layer);
            }
        }
        // The XDG user config is layer 2 — above every committed trail layer.
        if let Some(layer) = read_layer(user_config)? {
            layer_over(&mut merged, layer);
        }
        Value::Table(merged)
            .try_into()
            .map_err(|e| io::Error::other(format!("config does not resolve: {e}")))
    }
}

/// Read one `config/balls.toml` layer. Absent ⇒ `None` (the layer is empty, the
/// common un-configured case); malformed ⇒ an error naming the file; any other
/// read error propagates.
fn read_layer(path: &Path) -> io::Result<Option<Table>> {
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
/// or object) fully replaces its `base` entry.
fn layer_over(base: &mut Table, inner: Table) {
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
