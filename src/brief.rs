//! The landing's brief — `config/PRIME.md`, printed verbatim by every `prime`
//! (bl-c84f).
//!
//! The gap it closes: a fresh agent primes and learns nothing about THIS repo.
//! `prime` founds the substrate, converges skew and settles the store — and
//! prints no word about the project it just readied. The repo's hard-won rules
//! exist and are correct, but balls never hands them over; today an AGENTS.md
//! fills the hole only because the harness happens to load one. Point balls at
//! a repo whose harness does not, and the tracker says nothing.
//!
//! The whole mechanism: if `config/PRIME.md` exists on the landing, print it.
//! No verb, no field, no store, no flag — the brief is CONFIG, and core holds
//! only "print it if present." That is what keeps the severability test (§0)
//! satisfied: deleting the capability deletes a file, never a line of code.
//!
//! **Why the landing and not the project tree.** `install` is a pure path-copy
//! (§6, folder = mirror), so a brief committed to the landing rides
//! `bl prime --center <hub>` for free: enroll a checkout and it adopts the
//! project's brief along with its config, fleet-wide, from one authority.
//! Inert markdown is also the safest thing config can carry — all config is
//! potential RCE (§0), and this payload is text nothing executes.
//!
//! **The discipline that keeps it from rotting: PRIME.md POINTS, it does not
//! restate.** "Read `docs/architecture.md` §9 before touching close" — never a
//! copy of §9. A restated fact drifts from the thing it restates and nothing
//! corrects it, because the diff that invalidates it never touches the copy;
//! rot is worse than absence, since agents trust what they are handed. A
//! pointer cannot drift. It is also what stops the brief from becoming a second
//! home for AGENTS.md — two homes for one fact being the drift this project
//! refuses everywhere else (§0, single source of truth).
//!
//! Two subtractions are load-bearing, and both are the absence of something:
//! - **No seed.** `default-config/` ships no PRIME.md, so absence is silence. A
//!   seeded template would print boilerplate on every prime, forever, and train
//!   agents to skip the one thing it exists to make them read.
//! - **No flag.** It prints unconditionally, every prime. Any `--quiet` would be
//!   the smell (§0: a new flag is a smell); "too long to print each time" is the
//!   file's fault and is fixed in the file.

use std::fs;
use std::io;
use std::path::Path;

/// The brief's fixed name under the landing's `config/`.
const FILE: &str = "PRIME.md";

/// Read the landing's brief, if it has one. `Ok(None)` is the ordinary case —
/// no brief configured, nothing to say.
pub fn read(landing: &Path) -> io::Result<Option<String>> {
    let path = landing.join("config").join(FILE);
    // An unreadable brief is a real error and propagates: a configured brief
    // that cannot be shown is not the same state as no brief at all.
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(fs::read_to_string(path)?))
}

/// Print the brief verbatim to stdout. Verbatim means verbatim — no header, no
/// wrapper, no trailing newline of our own: what the file says is what the
/// agent reads, and the file owns its own shape.
pub fn emit(landing: &Path) -> io::Result<()> {
    if let Some(text) = read(landing)? {
        print!("{text}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "brief_tests.rs"]
mod brief_tests;
