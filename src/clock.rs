//! The op instant `T` — one clock read per op, three consumers (§8, bl-8b98).
//!
//! An op reads the wall clock exactly ONCE, here, and every timestamp it authors
//! derives from that single instant `T`: the frontmatter `created`/`updated` ints
//! ([`crate::mutate`]), core's own seal-commit date ([`crate::git`] sets
//! `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE`), and the delivery squash
//! ([`crate::plugin`] exports the same dates into every plugin's spawn env, so the
//! plugin's `commit-tree` inherits them). Before this the three agreed only by
//! luck — three independent reads landing in the same second — a
//! single-source-of-truth violation this dissolves.
//!
//! `T` resolves down a fail-open ladder:
//!
//! ```text
//! clock_provider  (a conf-set LOCAL value → a resolved bin; the product seam) >
//! BALLS_CLOCK      (an i64 env, the edge TEST seam — deterministic core tests) >
//! the system clock ([`crate::log::wall`]; the default — today's behaviour)
//! ```
//!
//! The provider is a DIRECTLY-SET local value (bl-cfe3): an absolute path, or a
//! PATH-resolved name, living in the per-machine LOCAL-TRUST layer (the per-clone
//! `binding.toml` / XDG, [`crate::config::clock_provider`]) and set by `bl conf`
//! — NOT a landing-config name bound via `install`. The clock is box-local (§1),
//! cosmetic, and fail-open, so it needs none of the shared-schedule/RCE-consent
//! machinery of the hook `bin/<name>` indirection, and it NEVER travels on
//! `install` (§4).
//!
//! The provider is the ONLY rung that can fail, and it is NON-FATAL: a value that
//! resolves to no binary, a non-zero exit, or unparseable output logs a note and
//! falls to the next rung. This is the deliberate asymmetry with hook dispatch,
//! where a dangling bin ABORTS (§6): a hook is load-bearing, the op clock is
//! cosmetic with a sane default, so it degrades instead of blocking. With NOTHING
//! set the ladder bottoms at the system clock — byte-identical to pre-bl-8b98
//! behaviour.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config;
use crate::edge::Edge;
use crate::log::wall;

/// The op instant plus an optional fail-open note. `note` is `Some` when a
/// configured provider could not be honoured; the seal path emits it through the
/// op [`crate::log::Log`] (never a bare `eprintln` — it must be threshold-gated
/// and persisted like every other op record, bl-bfcc).
pub struct Instant {
    /// Unix seconds — the §3 convention, stamped into frontmatter and every
    /// commit the op authors.
    pub t: i64,
    /// A fail-open diagnostic to log, or `None` when the ladder resolved cleanly.
    pub note: Option<String>,
}

/// Resolve the op instant for `edge` — read this checkout's LOCAL-TRUST
/// `clock_provider` (the per-clone `binding.toml` / XDG,
/// [`config::clock_provider`]) and run the fail-open [`resolve`] ladder,
/// resolving a named value against this box's binaries with [`locate`]. The
/// impure wrapper: it does the config + filesystem reads, while [`resolve`]
/// stays pure over its inputs (the provider string + a resolver closure) so the
/// ladder is unit-tested exhaustively.
pub fn for_op(edge: &Edge) -> io::Result<Instant> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let provider = config::clock_provider(&clone.binding(), &edge.xdg.user_config());
    Ok(resolve(provider.as_deref(), |name| locate(name, edge), edge.balls_clock, wall))
}

/// Resolve a `clock_provider` value to THIS box's binary (bl-cfe3): an absolute
/// path is used verbatim (when it is a file); any other value is a name resolved
/// beside `bl` first (the [`crate::seed`] sibling rule), then on `$PATH`
/// (`edge.path_dirs`) — the SAME "this machine" lookup a plugin binary gets, minus
/// the `bin/<name>` symlink, because the clock never travels on `install`. No hit
/// ⇒ `None`, and [`resolve`] falls open.
fn locate(value: &str, edge: &Edge) -> Option<PathBuf> {
    let p = Path::new(value);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    edge.exe_dir.iter().chain(edge.path_dirs.iter()).map(|d| d.join(value)).find(|p| p.is_file())
}

/// The fail-open ladder, pure over its inputs (`system` injected as a `fn`
/// pointer like [`crate::log::Log`]'s clock, so tests are deterministic and this
/// does no hidden time read; `locate` injected so the filesystem lookup is a
/// closure the tests stub). A `provider` value resolving to a bin that prints one
/// parseable unix-seconds line wins; ANY provider failure (unresolvable, non-zero
/// exit, unparseable) falls through carrying a note; then the `balls_clock` test
/// seam; then the `system` clock.
#[must_use]
pub fn resolve(provider: Option<&str>, locate: impl Fn(&str) -> Option<PathBuf>, balls_clock: Option<i64>, system: fn() -> i64) -> Instant {
    let fallback = |note| Instant { t: balls_clock.unwrap_or_else(system), note };
    let Some(value) = provider else {
        return fallback(None);
    };
    let Some(bin) = locate(value) else {
        return fallback(Some(format!(
            "clock_provider {value} not found (not an absolute path, not beside bl or on PATH) — using the system clock"
        )));
    };
    match probe(&bin) {
        Ok(t) => Instant { t, note: None },
        Err(e) => fallback(Some(format!("clock_provider {value}: {e} — using the system clock"))),
    }
}

/// Run a provider bin: one unix-seconds `i64` line on stdout, exit 0. Anything
/// else (non-zero exit, empty/non-integer output) is an error the caller turns
/// into a fail-open note. `retry_busy` absorbs the ETXTBSY a freshly-written bin
/// can throw under parallel test spawns (bl-6cd9).
fn probe(bin: &Path) -> io::Result<i64> {
    let child = crate::plugin::retry_busy(|| {
        Command::new(bin).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
    })?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(io::Error::other(format!("provider exited {}", out.status)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or_default().trim();
    line.parse::<i64>().map_err(|_| io::Error::other(format!("provider printed non-integer {line:?}")))
}

/// The `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` pair pinning a git commit to the op
/// instant `t`. `@<unix>` (no offset) fixes the INSTANT while letting git supply
/// the LOCAL timezone offset for display — byte-identical to how git stamps an
/// un-dated commit today, only the instant is now `t` instead of a fresh read.
/// The single home of this format, shared by core's seal ([`crate::git`]) and the
/// plugin spawn env ([`crate::plugin`]), so the store commit and the delivery
/// squash carry the SAME date.
#[must_use]
pub fn git_date_env(t: i64) -> [(&'static str, String); 2] {
    let date = format!("@{t}");
    [("GIT_AUTHOR_DATE", date.clone()), ("GIT_COMMITTER_DATE", date)]
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
