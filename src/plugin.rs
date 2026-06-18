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

use std::io::{self, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::lifecycle::{Plugins, Sealed};
use crate::log::{Level, Log};
use crate::message::{self, PROTOCOL};
use crate::op::Phase;
use crate::registry::PluginRef;
use crate::renames::renamed_to;
use crate::verb::Verb;
use crate::wire::{OpContext, SealFacts};

// The `protocol` self-describe and the subprocess IO plumbing live in siblings;
// re-exported so consumers keep reaching `crate::plugin::{describe, Protocol,
// retry_busy}` (install validates a binding, reads spawns the same binaries).
#[path = "plugin_protocol.rs"]
mod protocol;
pub use protocol::{describe, Protocol};

#[path = "plugin_io.rs"]
mod plugin_io;
pub(crate) use plugin_io::retry_busy;
use plugin_io::{capped_lines, feed, RELAY_LINE_MAX};

/// The built-in recursion cap (§6, bl-7110): reaching this depth ABORTS the op.
pub const DEPTH_CAP: u32 = 8;

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

    /// A schedule entry whose `bin/<name>` is missing. A RENAMED first-party
    /// plugin (§15, [`renamed_to`]) is SKIPPED with a notice — non-fatal, so an
    /// old committed schedule keeps working locally (just without that plugin)
    /// until its owner updates the names; any other missing plugin ABORTS the op
    /// (a genuine "referenced but not installed" misconfig). Returning `Ok` means
    /// the op proceeds as if the entry were absent.
    fn unbound(&self, name: &str, phase: Phase) -> io::Result<()> {
        let Some(current) = renamed_to(name) else {
            return Err(io::Error::other(format!(
                "plugin {name} referenced but bin/{name} missing — run bl install"
            )));
        };
        self.log.record(
            Level::Info,
            "core",
            Some(phase),
            &format!(
                "plugin {name} was renamed {current}; your config still references {name} — replace it with {current} in the [hooks] schedule (bl conf), then prime to resume"
            ),
        );
        Ok(())
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
        let Some(bin) = plugin.bin.as_ref() else {
            return self.unbound(&plugin.name, phase);
        };
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
        let status = self.spawn(bin, &plugin.name, op, phase, dir, &json)?;
        if status.success() {
            return Ok(());
        }
        // A non-zero exit yields an `error` record (the failure locus, surviving
        // any threshold — §6) and an [`io::Error`] that aborts the op. On a
        // rollback the record says what that failure MEANS instead — side effects
        // may not be unwound. §14 ignores the exit, so this record is the only
        // trace (a quiet surviving delivery squash is how bl-430e stayed hidden).
        let (name, o, p) = (&plugin.name, op.token(), phase.token());
        let msg = match rolling_back {
            Some(_) => format!("plugin {name} rollback failed ({status}) — its {o}.{p} side effects may not be unwound"),
            None => format!("plugin {name} aborted the op ({status})"),
        };
        self.log.record(Level::Error, "core", Some(phase), &msg);
        Err(io::Error::other(msg))
    }

    /// Spawn `<bin> <op> <phase>`: cwd `dir`, §6 env, `payload` on stdin, stdout
    /// INHERITED — forwarded to the invoker's stdout verbatim (§6, the plugin's
    /// user-facing channel: "claim prints the worktree path" is the plugin
    /// printing here); core PARSES NOTHING back (no return channel, §7). stderr is
    /// PIPED and enveloped into the unified log line-by-line (`src=<name>`,
    /// `lvl=info`). Core logs an `invoke` record first. Pure process mechanics:
    /// the exit status is returned for [`Subprocess::invoke`] to interpret (a
    /// forward abort and a failed rollback warrant different records).
    fn spawn(
        &self,
        bin: &Path,
        name: &str,
        op: Verb,
        phase: Phase,
        dir: &Path,
        payload: &str,
    ) -> io::Result<std::process::ExitStatus> {
        self.log.record(Level::Debug, "core", Some(phase), &format!("invoke {name}"));
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
        feed(child.stdin.take().expect("stdin was configured as a pipe"), payload)?;
        self.relay(name, phase, child.stderr.take().expect("stderr was configured as a pipe"));
        child.wait()
    }

    /// Envelope a plugin's piped stderr into the unified log, one record per line
    /// (`src=<name>`, `lvl=info`). Each line is read through [`capped_lines`] so a
    /// plugin emitting a blob with no newline cannot make the relay buffer
    /// unbounded memory (bl-2d6d) — `log` trims each record further still. A read
    /// error just ends the relay — logging is best-effort and must not abort the op.
    fn relay(&self, name: &str, phase: Phase, stderr: std::process::ChildStderr) {
        capped_lines(BufReader::new(stderr), RELAY_LINE_MAX, |line| {
            self.log.record(Level::Info, name, Some(phase), line);
        });
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
#[cfg(test)]
#[path = "plugin_feed_tests.rs"]
mod feed_tests;
#[cfg(test)]
#[path = "plugin_unbound_tests.rs"]
mod unbound_tests;
