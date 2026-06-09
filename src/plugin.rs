//! §6 plugin contract & dispatch — subprocess-uniform.
//!
//! A plugin is a single binary, invoked identically whether it is one of the
//! shipped capabilities or a third party: there is no in-process path and no
//! privileged plugin. balls spawns `<bin> <op> <phase>` with the §7 payload on
//! stdin and the §6 env set, and reads NOTHING back — a plugin contributes by
//! editing the change worktree, never by printing values (§7, no return
//! channel). The plugin stays DUMB about diagnostics too: it writes raw stderr
//! and is told nothing about where it lands (no `BALLS_LOG_DIR` — a new env is a
//! §0 smell); balls pipes the child's stderr and ENVELOPES each line as a record
//! into the unified op log (`src=<name>`, `lvl=info`). A non-zero exit aborts the
//! op — core emits an `error` record naming the locus first — and the
//! [`crate::lifecycle`] engine then rolls the prior plugins back in reverse.
//!
//! [`Subprocess`] is the production [`Plugins`] seam. It is built once per op
//! with the op-constant [`OpContext`] (the §7 wire data the verb layer authored),
//! the op's [`Log`] sink (it logs each `invoke` and envelopes plugin stderr), and
//! the recursion `depth` balls is running at. The engine hands it the per-phase
//! post-seal [`Sealed`] facts.
//!
//! **Recursion guard (§6, bl-7110).** A plugin may shell back to `bl`; every
//! nested call bumps `BALLS_PLUGIN_DEPTH`. Crossing [`DEPTH_CAP`] ABORTS the op —
//! fail, not silent: [`Subprocess::run`] returns an error naming the op/phase that
//! overran, so the [`crate::lifecycle`] engine rolls the prior plugins back in
//! reverse (§8/§14) and the runaway SURFACES. There is no hatch to re-enable
//! plugins on a nested call — that would let a runaway defeat its own backstop.
//! `rollback` cannot spawn at the cap either, so it no-ops (best-effort, §14).

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::lifecycle::{Plugins, Sealed};
use crate::log::{Level, Log};
use crate::message::{self, PROTOCOL};
use crate::op::Phase;
use crate::registry::PluginRef;
use crate::verb::Verb;
use crate::wire::{OpContext, SealFacts};

/// The built-in recursion cap (§6, bl-7110): reaching this depth ABORTS the op.
pub const DEPTH_CAP: u32 = 8;

/// Bounded retries for a transient `ETXTBSY` when exec'ing a plugin binary —
/// see [`retry_busy`].
const BUSY_RETRIES: u32 = 6;
const BUSY_BACKOFF_MS: u64 = 2;

/// Retry `exec` while it reports `ExecutableFileBusy`. Exec'ing a file any
/// process holds open for writing yields `ETXTBSY` — transient when a plugin
/// binary is being (re)written concurrently (a parallel agent's `bl install`,
/// §6). Bounded, then gives up with the last error.
fn retry_busy<T>(mut exec: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    for _ in 0..BUSY_RETRIES {
        match exec() {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(BUSY_BACKOFF_MS));
            }
            other => return other,
        }
    }
    exec()
}

/// A plugin's self-description from `<bin> protocol`: the protocol version(s) it
/// speaks and the ops it handles. balls never persists it — it is read at
/// install time to validate a binding, and is diagnostics otherwise (§6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protocol {
    pub protocol: Vec<u32>,
    pub ops: Vec<String>,
}

/// Accept a scalar `protocol: 1` or a list `protocol: [1, 2]` on the wire — both
/// are valid §6 self-descriptions ("version(s)").
#[derive(Deserialize)]
#[serde(untagged)]
enum Versions {
    One(u32),
    Many(Vec<u32>),
}

#[derive(Deserialize)]
struct RawProtocol {
    protocol: Versions,
    ops: Vec<String>,
}

impl Protocol {
    /// Does this plugin declare that it handles `op`? The install-time check.
    #[must_use]
    pub fn handles(&self, op: Verb) -> bool {
        self.ops.iter().any(|o| o == op.token())
    }

    /// Does this plugin speak protocol `version`? The install-time check.
    #[must_use]
    pub fn speaks(&self, version: u32) -> bool {
        self.protocol.contains(&version)
    }
}

/// Run `<bin> protocol` and parse its `{ protocol, ops }` self-description (§6).
/// A spawn failure, a non-zero exit, or unparseable JSON is an [`io::Error`].
pub fn describe(bin: &Path) -> io::Result<Protocol> {
    let out = retry_busy(|| Command::new(bin).arg("protocol").output())?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "plugin protocol self-describe exited {}",
            out.status
        )));
    }
    let raw: RawProtocol = serde_json::from_slice(&out.stdout).map_err(io::Error::other)?;
    let protocol = match raw.protocol {
        Versions::One(v) => vec![v],
        Versions::Many(vs) => vs,
    };
    Ok(Protocol { protocol, ops: raw.ops })
}

/// The production [`Plugins`] seam: spawns each plugin as a subprocess with the
/// §7 wire on stdin (§6). Borrows the op's [`Log`] for the run's lifetime — it
/// shares the one per-clone sink with core's own lifecycle records.
pub struct Subprocess<'a> {
    ctx: OpContext,
    log: &'a Log,
    depth: u32,
}

impl<'a> Subprocess<'a> {
    /// Build the dispatcher for one op: the §7 op-constant `ctx`, the op's `log`
    /// sink (shared with core's lifecycle records), and the recursion `depth`
    /// balls is running at (read from `BALLS_PLUGIN_DEPTH` by the binary edge; `0`
    /// at the top level).
    #[must_use]
    pub fn new(ctx: OpContext, log: &'a Log, depth: u32) -> Self {
        Self { ctx, log, depth }
    }

    /// True once balls is running AT the §6 invocation-tree cap: no further
    /// nested plugin may spawn. `run` ABORTS the op here (bl-7110); `rollback`
    /// no-ops (it cannot spawn either — best-effort, §14).
    fn at_cap(&self) -> bool {
        self.depth >= DEPTH_CAP
    }

    /// Assemble the §7 payload and spawn the plugin. `rolling_back` is `Some` for
    /// a rollback call (it tags the payload `rolling_back: pre|post`, §7).
    fn invoke(
        &self,
        plugin: &PluginRef,
        op: Verb,
        phase: Phase,
        dir: &Path,
        sealed: Option<&Sealed>,
        rolling_back: Option<&str>,
    ) -> io::Result<()> {
        let bin = plugin.bin.as_ref().ok_or_else(|| {
            let n = &plugin.name;
            io::Error::other(format!("plugin {n} referenced but bin/{n} missing — run bl install"))
        })?;
        // Parse the §5 trailers into the post wire's `metadata` (the engine
        // handed us the raw message — §5 lives on this side of the seam). A
        // diffless op (§13) seals no message, so `s.message` is `None` and the
        // facts go out commit-pair-only, metadata absent.
        let metadata = match sealed.and_then(|s| s.message) {
            Some(m) => Some(message::parse(m)?),
            None => None,
        };
        let facts = sealed.map(|s| SealFacts {
            commit: s.commit,
            previous_commit: s.previous_commit,
            metadata: metadata.as_ref(),
        });
        let payload = self.ctx.wire(&plugin.name, op.token(), phase.token(), facts, rolling_back);
        let json = serde_json::to_string(&payload).map_err(io::Error::other)?;
        self.spawn(bin, &plugin.name, op, phase, dir, &json)
    }

    /// Spawn `<bin> <op> <phase>`: cwd `dir`, §6 env, `payload` on stdin, stdout
    /// INHERITED — forwarded to the invoker's stdout verbatim (§6, the plugin's
    /// user-facing channel: "claim prints the worktree path" is the plugin
    /// printing here); core PARSES NOTHING back (no return channel, §7). stderr is
    /// PIPED and enveloped into the unified log line-by-line (`src=<name>`,
    /// `lvl=info`). Core logs an `invoke` record first. A non-zero exit yields an
    /// `error` record (the failure locus, surviving any threshold — §6) and an
    /// [`io::Error`] that aborts the op.
    fn spawn(
        &self,
        bin: &Path,
        name: &str,
        op: Verb,
        phase: Phase,
        dir: &Path,
        payload: &str,
    ) -> io::Result<()> {
        self.log.record(Level::Info, "core", Some(phase), &format!("invoke {name}"));
        let depth = (self.depth + 1).to_string();
        let mut child = retry_busy(|| {
            Command::new(bin)
                .arg(op.token())
                .arg(phase.token())
                .current_dir(dir)
                .env("BALLS_PROTOCOL", PROTOCOL.to_string())
                .env("BALLS_PLUGIN_NAME", name)
                .env("BALLS_PLUGIN_DEPTH", &depth)
                .stdin(Stdio::piped())
                .stdout(Stdio::inherit())
                .stderr(Stdio::piped())
                .spawn()
        })?;
        child.stdin.take().expect("stdin was configured as a pipe").write_all(payload.as_bytes())?;
        self.relay(name, phase, child.stderr.take().expect("stderr was configured as a pipe"));
        let status = child.wait()?;
        if status.success() {
            Ok(())
        } else {
            self.log.record(Level::Error, "core", Some(phase), &format!("plugin {name} aborted the op ({status})"));
            Err(io::Error::other(format!("plugin {name} aborted the op ({status})")))
        }
    }

    /// Envelope a plugin's piped stderr into the unified log, one record per line
    /// (`src=<name>`, `lvl=info`). The final non-newline-terminated blob, if any,
    /// is read to EOF as one line ([`BufRead::lines`] yields it). A read error
    /// just ends the relay — logging is best-effort and must not abort the op.
    fn relay(&self, name: &str, phase: Phase, stderr: std::process::ChildStderr) {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            self.log.record(Level::Info, name, Some(phase), &line);
        }
    }
}

impl Plugins for Subprocess<'_> {
    fn run(&self, plugin: &PluginRef, op: Verb, phase: Phase, dir: &Path, sealed: Option<&Sealed>) -> io::Result<()> {
        if self.at_cap() {
            let msg = format!(
                "invocation-tree depth cap ({DEPTH_CAP}) reached at {}.{} — aborting before plugin {} (§6)",
                op.token(),
                phase.token(),
                plugin.name
            );
            self.log.record(Level::Error, "core", Some(phase), &msg);
            return Err(io::Error::other(msg));
        }
        self.invoke(plugin, op, phase, dir, sealed, None)
    }

    fn rollback(&self, plugin: &PluginRef, op: Verb, phase: Phase, dir: &Path, sealed: Option<&Sealed>) {
        if self.at_cap() {
            return;
        }
        // The undone phase IS the `rolling_back` tag (§7); exit is ignored (§14).
        let _ = self.invoke(plugin, op, phase, dir, sealed, Some(phase.token()));
    }
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
