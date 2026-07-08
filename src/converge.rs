//! §12/§15 prime's rename convergence (bl-18bf piece 1) — the one MUTATION
//! prime performs to close version skew.
//!
//! `bl doctor` was built and burned (bl-77a7): every fix is an existing
//! idempotent verb, drift fails loud at point of use. That held for STEADY-STATE
//! drift; it never covered VERSION SKEW — a checkout healthy under an old binary
//! and stale under a new one. The catalog's lone semantics-preserving case is a
//! retired first-party plugin name (`renames::renamed_to`, the closed static map
//! — today `tracker` → `bl-tracker`, bl-27bf) still committed in a landing's
//! `config/plugins.toml`. The dispatch notice (bl-27bf, [`crate::plugin`]) was an
//! explicit stopgap "until its owner updates it"; prime acting as the owner's
//! hand IS that update — spelling correction on a closed, first-party-RESERVED
//! (§5/§6) map, never policy change (a retired name has exactly one meaning and
//! can never bind again).
//!
//! Convergence is [`converge`], called from [`crate::checkout::prime`] at ONE
//! site — after landing founding/rebind (and any adopt), before the prime chain's
//! [`crate::hooks::Hooks::effective`] read — so the op that rewrites also
//! dispatches the rewritten schedule and binds the fresh name in one breath (no
//! "run prime twice"). It:
//!
//! 1. rewrites every retired name across the `[hooks]` arrays AND `[source]`
//!    keys through [`crate::conf::edit_landing_toml`] (a raw-`toml::Table` seal
//!    that round-trips a team's foreign tables — NOT `Hooks::to_toml`, which
//!    drops them), one ordinary landing commit, no-change = no commit;
//! 2. binds the CURRENT name to its sibling beside `bl` (the seed's own rule,
//!    [`crate::seed::sibling`]) and drops the now-dangling old-name symlink (a
//!    dangling link is not work — the one deletion converge is allowed).
//!
//! **Live-binding guard.** `bl-` is reserved but `tracker` is NOT, so a third
//! party may legitimately ship a live-bound `tracker`. Converge acts only on an
//! UNBOUND old name (`Registry::resolve_bin(old).is_none()`): a live-bound old
//! name is not our retired plugin, so its schedule entry and symlink are left
//! whole and dispatch invokes it as ever.
//!
//! **Cost (§the high-throughput constraint).** On the overwhelmingly common
//! converged checkout: ONE extra read+parse of the landing `plugins.toml` and a
//! scan against a static map — zero new subprocess spawns, zero new commits. Git
//! runs only when a retired name is actually present-and-unbound (once per
//! checkout, ever). Boundaries: LANDING only — an old name in the XDG layer is
//! the user's file (the dispatch notice stays its cover); adopt paths converge
//! their COPY-IN ([`rewrite_config`]) so a stale center cannot re-inject the old
//! name each cycle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;
use toml::value::{Table, Value};

use crate::registry::Registry;
use crate::{config, conf, renames, seed};

/// Converge a version-skewed landing (§12.1): rewrite every retired-and-unbound
/// first-party name in the committed `config/plugins.toml` schedule to its
/// current spelling, seal ONE commit, then bind the current name to its sibling
/// beside `bl` and drop the dangling old symlink. `exe_dir` is the directory
/// holding `bl` (where the renamed sibling ships). A converged checkout no-ops:
/// no retired name present-and-unbound ⇒ no read past the parse, no git, no bind.
pub fn converge(landing: &Path, exe_dir: Option<&Path>, actor: &str) -> io::Result<()> {
    let plugins = landing.join("config").join("plugins.toml");
    let root = config::read_layer(&plugins)?.unwrap_or_default();
    let reg = Registry::at(landing);
    let map = pending(&root, &reg);
    if map.is_empty() {
        return Ok(()); // converged — the whole clean-path budget is the read above
    }
    conf::edit_landing_toml(landing, actor, "plugins.toml", &subject(&map), |root| {
        apply(root, &map);
        Ok(())
    })?;
    // Finish the seed's rule: bind the current name to its sibling beside `bl`
    // (absent ⇒ leave unbound; the ordinary [source]-hinted refusal covers it,
    // `bl install` fixes it), then drop the now-dangling old-name symlink so the
    // rewrite does not turn a non-fatal skip-with-notice into a hard abort.
    for (old, current) in &map {
        if let Some(bin) = seed::sibling(exe_dir, current) {
            reg.bind(current, &bin)?;
        }
        reg.unbind(old)?;
    }
    Ok(())
}

/// Apply the rename map to config a center is COPYING INTO a landing (adopt's
/// `--install`/`--center` copy-in, [`crate::adopt`]) BEFORE the seal — so a stale
/// center's `tracker` lands already spelled `bl-tracker` and re-adopting is the
/// no-op seal, not a rewrite commit each cycle. `change` is the staged change
/// worktree; `reg` is the landing's registry (the same live-binding guard). No
/// retired name present ⇒ the copied bytes are LEFT UNTOUCHED — no reserialize,
/// so identical adopted config still seals to nothing (§13).
pub(crate) fn rewrite_config(change: &Path, reg: &Registry) -> io::Result<()> {
    let plugins = change.join("config").join("plugins.toml");
    let Some(mut root) = config::read_layer(&plugins)? else {
        return Ok(()); // the copied config carries no schedule — nothing to canonicalize
    };
    let map = pending(&root, reg);
    if map.is_empty() {
        return Ok(());
    }
    apply(&mut root, &map);
    fs::write(&plugins, toml::to_string(&Value::Table(root)).expect("a plugins table always serializes"))
}

/// The retired-and-unbound first-party names present in this raw `plugins.toml`
/// root, mapped old → current. A name qualifies iff it is in the closed rename
/// map ([`renames::renamed_to`]) AND unbound here (`resolve_bin` is `None`, the
/// live-binding guard — a bound old name is a third party's, left whole).
fn pending(root: &Table, reg: &Registry) -> BTreeMap<String, &'static str> {
    let mut map = BTreeMap::new();
    for name in candidates(root) {
        if let Some(current) = renames::renamed_to(&name) {
            if reg.resolve_bin(&name).is_none() {
                map.insert(name, current);
            }
        }
    }
    map
}

/// Every plugin name this raw root references — string entries of the `[hooks]`
/// arrays and the `[source]` table keys (the two surfaces a retired name can
/// hide in). Deduped; any other shape contributes nothing.
fn candidates(root: &Table) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(Value::Table(hooks)) = root.get("hooks") {
        for value in hooks.values() {
            for entry in value.as_array().into_iter().flatten() {
                if let Some(name) = entry.as_str() {
                    names.insert(name.to_string());
                }
            }
        }
    }
    if let Some(Value::Table(source)) = root.get("source") {
        for name in source.keys() {
            names.insert(name.clone());
        }
    }
    names
}

/// Rewrite the retired names in-place: each `[hooks]` array entry is replaced by
/// its current spelling, and each `[source]` key is re-keyed (its hint carried
/// verbatim). Foreign tables and every other entry are untouched.
fn apply(root: &mut Table, map: &BTreeMap<String, &'static str>) {
    if let Some(Value::Table(hooks)) = root.get_mut("hooks") {
        for (_, value) in hooks.iter_mut() {
            if let Some(array) = value.as_array_mut() {
                for entry in array.iter_mut() {
                    if let Some(current) = entry.as_str().and_then(|n| map.get(n)) {
                        *entry = Value::String((*current).to_string());
                    }
                }
            }
        }
    }
    if let Some(Value::Table(source)) = root.get_mut("source") {
        for (old, current) in map {
            if let Some(hint) = source.remove(old.as_str()) {
                source.insert((*current).to_string(), hint);
            }
        }
    }
}

/// The landing-commit subject naming each rename applied (deterministic, from the
/// [`BTreeMap`] order): `balls: converge tracker->bl-tracker`.
fn subject(map: &BTreeMap<String, &'static str>) -> String {
    let pairs: Vec<String> = map.iter().map(|(old, current)| format!("{old}->{current}")).collect();
    format!("balls: converge {}", pairs.join(" "))
}

#[cfg(test)]
#[path = "converge_tests.rs"]
mod tests;
