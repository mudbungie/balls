//! §12 the federation trail — a pure walk over LOCAL checkouts.
//!
//! A trail is `landing → step(s) → terminus`: each checkout's committed
//! `next:` pointer names the next hop, so the walk is transitive and the
//! terminus is simply the last node reached. The walk is **stop-on-revisit**
//! (a pointer cycle, or two trails converging on one center, terminates rather
//! than loops) and has no depth cap (§12).
//!
//! This is the CORE half of the SEAM (spec bl-2e26 §12): "core walks the trail
//! over LOCAL checkouts (knows nothing of remotes); the tracker translates
//! remote↔local." So [`walk`] never fetches — it follows a `next` resolver the
//! caller supplies, which maps a local checkout to the next LOCAL checkout the
//! tracker has already materialized. In stealth (trail length 1) and before any
//! intermediate hop is materialized, `next` yields `None` and the walk is just
//! the landing — identical code, no special case. Materializing further hops so
//! the walk has more to traverse is the tracker's job (follow-up bl-02c3).

use std::path::{Path, PathBuf};

/// Walk the trail from `start`, following `next` to each subsequent local
/// checkout until it yields `None` (the terminus) or names a node already
/// visited (stop-on-revisit). Returns the ordered checkouts, landing first and
/// terminus last — so `bl sync` takes the last to find the task store, and
/// config VALUES would layer innermost-first over the whole list (§4/§12).
///
/// `next` is a `&mut dyn` trait object, not a generic, so the one compiled
/// `walk` is fully exercised by the tests (a generic monomorphizes per closure
/// and the production instantiation reads uncovered — the bl-ad4b lesson).
#[must_use]
pub fn walk(start: PathBuf, next: &mut dyn FnMut(&Path) -> Option<PathBuf>) -> Vec<PathBuf> {
    let mut trail = vec![start.clone()];
    let mut current = start;
    while let Some(step) = next(current.as_path()) {
        if trail.contains(&step) {
            break; // stop-on-revisit: a cycle or a convergence, not an error
        }
        trail.push(step.clone());
        current = step;
    }
    trail
}

#[cfg(test)]
#[path = "trail_tests.rs"]
mod tests;
