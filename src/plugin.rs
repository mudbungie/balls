//! §6 plugin contract & dispatch — subprocess-uniform.
//!
//! A plugin is a single binary, invoked identically whether it is one of the
//! shipped capabilities or a third party: there is no in-process path and no
//! privileged plugin. balls spawns `<bin> <op> <phase>` with the §7 payload on
//! stdin and the §6 env set, captures stderr to `logs/<name>/`, and reads
//! NOTHING back — a plugin contributes by editing the change worktree, never by
//! printing values (§7, no return channel). A non-zero exit aborts the op; the
//! [`crate::lifecycle`] engine then rolls the prior plugins back in reverse.
//!
//! [`Subprocess`] is the production [`Plugins`] seam. It is built once per op
//! with the op-constant [`OpContext`] (the §7 wire data the verb layer authored),
//! the checkout's `logs/` root, and the recursion `depth` balls is running at.
//! The engine hands it the per-phase post-seal [`Sealed`] facts.
//!
//! **Recursion guard (§6).** A plugin may shell back to `bl`; every nested call
//! bumps `BALLS_PLUGIN_DEPTH`. Once balls is itself running at [`DEPTH_CAP`] it
//! runs the op PLUGIN-FREE — suppressed, not refused — so a chain can never
//! cascade without bound. A plugin that wants nested plugins re-enables them on
//! its own nested call.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::lifecycle::{Plugins, Sealed};
use crate::message::{self, PROTOCOL};
use crate::op::Phase;
use crate::registry::PluginRef;
use crate::verb::Verb;
use crate::wire::{OpContext, SealFacts};

/// The built-in recursion cap (§6): at this depth, nested ops run plugin-free.
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
/// §7 wire on stdin (§6).
pub struct Subprocess {
    ctx: OpContext,
    logs: PathBuf,
    depth: u32,
}

impl Subprocess {
    /// Build the dispatcher for one op: the §7 op-constant `ctx`, the checkout's
    /// `logs` root, and the recursion `depth` balls is running at (read from
    /// `BALLS_PLUGIN_DEPTH` by the binary edge; `0` at the top level).
    #[must_use]
    pub fn new(ctx: OpContext, logs: &Path, depth: u32) -> Self {
        Self { ctx, logs: logs.to_path_buf(), depth }
    }

    /// At the cap, the whole op runs plugin-free (§6) — `run`/`rollback` no-op.
    fn suppressed(&self) -> bool {
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

    /// Spawn `<bin> <op> <phase>`: cwd `dir`, §6 env, `payload` on stdin, stderr
    /// to `logs/<name>/`, stdout discarded (no return channel, §7). A non-zero
    /// exit is an [`io::Error`].
    fn spawn(
        &self,
        bin: &Path,
        name: &str,
        op: Verb,
        phase: Phase,
        dir: &Path,
        payload: &str,
    ) -> io::Result<()> {
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
                .stdout(Stdio::null())
                .stderr(self.open_log(name, op, phase)?)
                .spawn()
        })?;
        child.stdin.take().expect("stdin was configured as a pipe").write_all(payload.as_bytes())?;
        let status = child.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("plugin {name} aborted the op ({status})")))
        }
    }

    /// Open the append-mode `logs/<name>/<op>-<phase>.log` sink for stderr (§6).
    fn open_log(&self, name: &str, op: Verb, phase: Phase) -> io::Result<fs::File> {
        let dir = self.logs.join(name);
        fs::create_dir_all(&dir)?;
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("{}-{}.log", op.token(), phase.token())))
    }
}

impl Plugins for Subprocess {
    fn run(&self, plugin: &PluginRef, op: Verb, phase: Phase, dir: &Path, sealed: Option<&Sealed>) -> io::Result<()> {
        if self.suppressed() {
            return Ok(());
        }
        self.invoke(plugin, op, phase, dir, sealed, None)
    }

    fn rollback(&self, plugin: &PluginRef, op: Verb, phase: Phase, dir: &Path, sealed: Option<&Sealed>) {
        if self.suppressed() {
            return;
        }
        // The undone phase IS the `rolling_back` tag (§7); exit is ignored (§14).
        let _ = self.invoke(plugin, op, phase, dir, sealed, Some(phase.token()));
    }
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
