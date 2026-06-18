//! Renamed first-party plugins (§6/§15): the closed, static map of a retired
//! plugin name to its current one.
//!
//! Consulted at dispatch ([`crate::plugin::Subprocess::invoke`]): a schedule
//! entry naming a former, now-unbound plugin is SKIPPED with a notice rather than
//! aborting the op — the self-diagnosing break (bl-27bf) that lets an old
//! committed schedule keep working (locally) until its owner updates it, instead
//! of bricking every plugin-dispatching verb (the rename binary is gone, so the
//! old name no longer binds, and `tracker` rode `prime.pre`/`install.pre` — the
//! very verbs that would fix it).
//!
//! It is a DIAGNOSTIC, not an alias: it never resolves the old name to the new
//! binary (an alias would be a permanent dialect, §16). Deleting an entry here
//! merely downgrades that name to the generic "referenced but not installed"
//! abort — so the map is SEVERABLE and can be pruned once no live config can
//! still carry the old name.
//!
//! - `tracker` → `bl-tracker` (bl-27bf): the lone first-party holdout brought
//!   under the `bl-` reservation (§5/§6 — `bl-` is RESERVED to first-party;
//!   `bl-delivery` already conformed). Seeded configs from balls ≤ 0.5.3 name it.

/// The current name a retired plugin `name` was renamed to, or `None` if `name`
/// was never renamed away.
#[must_use]
pub fn renamed_to(name: &str) -> Option<&'static str> {
    match name {
        "tracker" => Some("bl-tracker"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "renames_tests.rs"]
mod tests;
