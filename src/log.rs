//! §1/§4/§6 the unified per-clone op log — one JSON-lines sink per clone.
//!
//! balls owns ONE log file per clone bundle ([`crate::layout::CloneDir::op_log`]
//! — `clones/<enc>/log`), not a per-plugin or per-op-phase tree. Every record is
//! one JSON object on its own line `{ts, lvl, src, op, phase, msg}`, with `src`
//! either `core` (balls' own lifecycle records — begin/invoke/seal/abort) or a
//! plugin name (balls envelopes each line of that plugin's stderr, §6). The
//! source is a stamped FIELD, so a reader greps one source or reads the whole
//! interleaved sequence; metrics (§6) are a query over it, never core state.
//!
//! The log is LOCAL runtime state — gitignored, never committed (like
//! `binding.toml`), no rotation/retention (stale-but-harmless like an orphan
//! worktree; the [`Level`] threshold limits volume instead). One object per line
//! keeps concurrent appends from parallel agents atomic for sub-`PIPE_BUF`
//! writes (`O_APPEND`); bounding a giant enveloped line is bl-e6a0.
//!
//! A single threshold ([`Log::record`]) gates BOTH file persistence and the
//! terminal echo: a record below it is emitted nowhere. The threshold is the §4
//! `log_level` (CLI `--log-level` ▸ XDG ▸ landing ▸ serde-default `info`).
//! Logging is best-effort — it never aborts an op, so I/O errors are swallowed.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::op::Phase;
use crate::verb::Verb;

/// The §4 severity ladder. Ordered `Debug < Info < Error` so a record is emitted
/// iff its level is `>=` the configured threshold. Core lifecycle records and
/// enveloped plugin stderr are `Info`; read-op narration is `Debug` (default
/// `Info` keeps it out); a plugin's non-zero exit is `Error` (it outranks every
/// threshold, so the failure locus always lands — §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Error,
}

impl Level {
    /// The wire token written as `lvl`.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Error => "error",
        }
    }

    /// Parse a §4 `log_level` string. An unrecognised value is lenient — it reads
    /// as `Info`, the default threshold (a typo never silences the log).
    #[must_use]
    pub fn parse(s: &str) -> Level {
        match s {
            "debug" => Level::Debug,
            "error" => Level::Error,
            _ => Level::Info,
        }
    }
}

/// One JSON-lines record. `phase` is absent on an op-level line (begin/seal/
/// abort); present on a per-plugin `invoke`/envelope line.
#[derive(Serialize)]
struct Record<'a> {
    ts: i64,
    lvl: &'a str,
    src: &'a str,
    op: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<&'a str>,
    msg: &'a str,
}

/// The op-scoped log sink: the per-clone file path, the §4 threshold, the op
/// every record is stamped with, and an injected clock (so tests are
/// deterministic and the library does no hidden time read — [`wall`] is the
/// production clock).
pub struct Log {
    path: PathBuf,
    threshold: Level,
    op: Verb,
    now: fn() -> i64,
}

impl Log {
    /// Build the sink for one op. `path` is [`crate::layout::CloneDir::op_log`].
    #[must_use]
    pub fn new(path: PathBuf, threshold: Level, op: Verb, now: fn() -> i64) -> Self {
        Self { path, threshold, op, now }
    }

    /// Emit one record at `lvl` from `src` (`core` or a plugin name), tagged with
    /// the op and optional `phase`. Below threshold ⇒ nothing, anywhere. Otherwise
    /// the JSON line is appended to the file (best-effort `O_APPEND`) AND echoed to
    /// stderr — the single threshold gates both (§4). Never errors: logging must
    /// not abort an op, so a failed open/write is swallowed. Crate-internal (the
    /// log sink is not a public interface; `Log` is `pub` only for the dispatcher
    /// signature).
    pub(crate) fn record(&self, lvl: Level, src: &str, phase: Option<Phase>, msg: &str) {
        if lvl < self.threshold {
            return;
        }
        let record = Record {
            ts: (self.now)(),
            lvl: lvl.token(),
            src,
            op: self.op.token(),
            phase: phase.map(Phase::token),
            msg,
        };
        let mut line = serde_json::to_string(&record).expect("a flat record serializes");
        line.push('\n');
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = file.write_all(line.as_bytes());
        }
        eprint!("{line}");
    }
}

/// The production clock: unix seconds, the §3 time convention. Injected into a
/// [`Log`] so the sink itself stays time-free and unit-testable. A pre-epoch
/// clock — never, in practice — reads 0.
#[must_use]
pub fn wall() -> i64 {
    #[allow(clippy::cast_possible_wrap)]
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
