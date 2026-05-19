//! Advisory `min_bl_version` check (SPEC §5 / §10).
//!
//! Purely advisory. Older `bl` clients ignore the field structurally —
//! `Config` carries no `deny_unknown_fields`, so a pre-this-spec binary
//! drops it on load and there is nothing to engineer. A client that
//! *does* know the field warns when its own build is below the floor a
//! repo requests. This is the documented (not engineered) mitigation
//! for the single accepted BC caveat: an old `bl` on a deferred-mode
//! repo still local-squashes on `bl review`, contaminating the
//! integration branch (epic bl-00ca ACCEPTED CAVEAT 2026-05-10). We
//! nudge; we do not prevent.
//!
//! No semver crate: `bl` versions are plain `MAJOR.MINOR.PATCH` and a
//! three-int compare suffices. Operator shorthand (`0.4` ⇒ `0.4.0`) is
//! accepted; anything else is reported as malformed and the advisory is
//! ignored — an ineffective floor the operator should hear about.

/// The version this `bl` binary was built at.
pub fn running_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Parse `MAJOR[.MINOR[.PATCH]]` into a comparable triple. Absent
/// trailing components default to `0`. `None` for more than three
/// components or any non-numeric component (covers the empty string,
/// which splits to a single unparseable element).
fn parse_triple(v: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut nums = [0u64; 3];
    for (i, p) in parts.iter().enumerate() {
        nums[i] = p.parse::<u64>().ok()?;
    }
    Some((nums[0], nums[1], nums[2]))
}

/// Pure core: given the configured floor and the running version,
/// return the line to print, or `None` when nothing should be said.
/// Split from I/O so every branch is unit-testable without capturing
/// stderr. A floor that won't parse yields its own advisory; a running
/// version that won't parse silently says nothing (can't compare).
pub fn advisory(min: Option<&str>, running: &str) -> Option<String> {
    let min = min?;
    let Some(floor) = parse_triple(min) else {
        return Some(format!(
            "warning: config min_bl_version {min:?} is not a valid \
             MAJOR.MINOR.PATCH version; advisory ignored"
        ));
    };
    let here = parse_triple(running)?;
    if here < floor {
        Some(format!(
            "warning: this bl is {running}; this repo requests \
             min_bl_version {min}. An older bl on a deferred-mode repo \
             local-squashes `bl review` and contaminates the \
             integration branch (SPEC §10). Upgrade bl. (advisory only)"
        ))
    } else {
        None
    }
}

/// Emit the advisory to stderr if there is one. The only side-effecting
/// entry point; `Config::load` calls it after a successful parse so the
/// nudge rides every command that touches a configured repo.
pub fn warn_if_below(min: Option<&str>) {
    if let Some(msg) = advisory(min, running_version()) {
        eprintln!("{msg}");
    }
}

#[cfg(test)]
#[path = "min_version_tests.rs"]
mod tests;
