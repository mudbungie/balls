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
//! keeps concurrent appends from parallel agents atomic: every line is held at or
//! below [`LINE_MAX`] (`PIPE_BUF`) bytes so a single `O_APPEND` write never
//! interleaves with another agent's. The only unbounded field is the enveloped
//! plugin-stderr `msg` (a stack trace, a long git error, a diff line); it is
//! truncated with a [`TRUNC_MARKER`] so the whole record fits (§1, bl-e6a0).
//!
//! A single threshold ([`Log::record`]) gates BOTH file persistence and the
//! terminal echo: a record below it is emitted nowhere. The threshold is the §4
//! `log_level` (CLI `--log-level` ▸ XDG ▸ landing ▸ serde-default `info`).
//! Logging is best-effort — it never aborts an op, so I/O errors are swallowed.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::op::Phase;
use crate::verb::Verb;

/// The atomic-append bound: a single `write()` of at most this many bytes to a
/// regular file is not interleaved with a concurrent append (POSIX `PIPE_BUF`,
/// 4096 on Linux). Holding every log line at or below it is what makes the §1
/// lock-free `O_APPEND` claim true under parallel agents.
const LINE_MAX: usize = 4096;

/// Appended to a `msg` truncated to honour [`LINE_MAX`] — marks the line lossy so
/// a reader knows the plugin stderr (or core message) was longer.
const TRUNC_MARKER: &str = "…[truncated]";

/// The §4 severity ladder. Ordered `Debug < Info < Error` so a record is emitted
/// iff its level is `>=` the configured threshold. Severity classifies the VOICE,
/// not the op kind (bl-cf39): core narration (begin/seal/done/invoke, reads and
/// mutates alike) is `Debug` — the default `Info` keeps routine ops quiet — a
/// plugin speaking (enveloped stderr) is `Info`; a plugin's non-zero exit and a
/// core abort are `Error` (outranking every threshold, so the failure locus
/// always lands — §6).
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

    /// Parse a §4 `log_level` string STRICTLY: an unrecognised value (say `warn` —
    /// the ladder has no such rung) is a usage error naming the value and the
    /// ladder, never a silent fallback to the default (fail loud beats
    /// lenient-wrong, bl-56f7). Every `log_level` source — the CLI `--log-level`
    /// override and the config-sourced value alike — comes through here, so a typo
    /// in either fails the op instead of quietly running at `info`.
    pub fn parse(s: &str) -> io::Result<Level> {
        match s {
            "debug" => Ok(Level::Debug),
            "info" => Ok(Level::Info),
            "error" => Ok(Level::Error),
            _ => Err(io::Error::other(format!("unrecognised log level '{s}' — valid levels are debug|info|error"))),
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
        let line = self.line(lvl, src, phase, msg);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = file.write_all(line.as_bytes());
        }
        eprint!("{line}");
    }

    /// Serialize one record to its JSON line (newline-terminated), bounding the
    /// whole line to [`LINE_MAX`] so the `O_APPEND` write stays atomic (§1). The
    /// common path serializes once and returns; an oversized line (a long
    /// enveloped plugin-stderr `msg`) re-serializes with `msg` truncated on a
    /// `char` boundary plus a [`TRUNC_MARKER`], shrinking until it fits. Measuring
    /// the real serialized length each step accounts for JSON escaping without
    /// reimplementing serde's escape table (the single source of that truth).
    fn line(&self, lvl: Level, src: &str, phase: Option<Phase>, msg: &str) -> String {
        let ts = (self.now)();
        let mk = |m: &str| {
            let record = Record {
                ts,
                lvl: lvl.token(),
                src,
                op: self.op.token(),
                phase: phase.map(Phase::token),
                msg: m,
            };
            let mut line = serde_json::to_string(&record).expect("a flat record serializes");
            line.push('\n');
            line
        };
        let full = mk(msg);
        if full.len() <= LINE_MAX {
            return full;
        }
        // Oversized — a long enveloped plugin-stderr line. Shrink `msg` (the only
        // unbounded field) until the whole line fits, stepping down by the real
        // overflow and re-serializing each step so JSON escaping and `char`
        // boundaries are honoured without reimplementing serde's escape table.
        let mut keep = msg.len();
        loop {
            while keep > 0 && !msg.is_char_boundary(keep) {
                keep -= 1;
            }
            let line = mk(&format!("{}{TRUNC_MARKER}", &msg[..keep]));
            if line.len() <= LINE_MAX || keep == 0 {
                return line;
            }
            keep = keep.saturating_sub(line.len() - LINE_MAX);
        }
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
