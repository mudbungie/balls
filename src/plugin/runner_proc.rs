//! Subprocess plumbing for `Plugin`, split out of `runner.rs`.
//!
//! `runner.rs` stays scoped to the plugin *operations* (auth-check,
//! push, sync, describe, propose — the plugin contract). This file is
//! the *how*: spawn the child with stdin/stdout/stderr piped plus the
//! FD-3 diagnostics channel, decode a `PluginOutcome` into a typed
//! response, render diagnostics, and build the `Command`. It is a
//! second `impl Plugin` block — inherent methods, so `runner.rs`'s
//! `self.spawn_and_run(...)` call sites are unchanged.

use super::diag::prepare_diag_pipe;
use super::limits::{self, run_with_limits, PluginOutcome};
use super::runner::Plugin;
use super::types::PluginDiagnostic;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

impl Plugin {
    /// Spawn the plugin with stdin/stdout/stderr piped and a
    /// diagnostics channel on FD 3 (advertised via BALLS_DIAG_FD).
    /// Pipes the given bytes on stdin, runs under the usual limits,
    /// and returns the outcome including any diagnostics the plugin
    /// wrote. The child becomes the sole writer of the diag pipe
    /// after we drop our write end, so the pipe EOFs on exit.
    pub(crate) fn spawn_and_run(
        &self,
        subcmd: &str,
        extra_flags: &[(&str, &str)],
        stdin_bytes: &[u8],
    ) -> crate::error::Result<PluginOutcome> {
        let mut cmd = self.command(subcmd, extra_flags);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let diag = prepare_diag_pipe(&mut cmd)?;
        let child: Child = cmd.spawn()?;
        drop(diag.write);
        run_with_limits(child, stdin_bytes, Some(diag.read))
    }

    /// Parse newline-delimited JSON diagnostics from the plugin and
    /// print each record to stderr. Malformed lines are surfaced as a
    /// single warning and do not abort the rest.
    pub(crate) fn render_diagnostics(&self, op: &str, diag_bytes: &[u8]) {
        if diag_bytes.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(diag_bytes);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<PluginDiagnostic>(line) {
                Ok(d) => {
                    let code = d.code.as_deref().map(|c| format!(" [{c}]")).unwrap_or_default();
                    eprintln!(
                        "plugin {} {} {}{}: {}",
                        self.executable, op, d.level, code, d.message
                    );
                    if let Some(hint) = d.hint {
                        eprintln!("  hint: {hint}");
                    }
                    if let Some(task_id) = d.task_id {
                        eprintln!("  task: {task_id}");
                    }
                }
                Err(e) => {
                    eprintln!(
                        "warning: plugin `{}` {op} emitted invalid diagnostic JSON: {e}",
                        self.executable
                    );
                }
            }
        }
    }

    /// Build a `Command` for a plugin subcommand. Puts the child in
    /// its own process group so the timeout path can kill the whole
    /// subtree.
    pub(crate) fn command(&self, subcmd: &str, extra_flags: &[(&str, &str)]) -> Command {
        let mut cmd = Command::new(&self.executable);
        cmd.arg(subcmd);
        for (flag, value) in extra_flags {
            cmd.arg(flag).arg(value);
        }
        cmd.arg("--config")
            .arg(&self.config_path)
            .arg("--auth-dir")
            .arg(&self.auth_dir);
        cmd.process_group(0);
        cmd
    }

    pub(crate) fn parse_outcome<T: for<'de> serde::Deserialize<'de>>(
        &self,
        op: &str,
        outcome: PluginOutcome,
    ) -> Option<T> {
        self.render_diagnostics(op, &outcome.diagnostics);
        let exe = &self.executable;
        if outcome.timed_out {
            let secs = limits::timeout().as_secs();
            eprintln!("warning: plugin `{exe}` {op} timed out after {secs}s, killed");
            return None;
        }
        if outcome.truncated {
            let cap = limits::effective_stream_cap();
            eprintln!("warning: plugin `{exe}` {op} exceeded {cap} bytes of stdout, discarding (raise BALLS_PLUGIN_ABS_MAX_STREAM_BYTES if a real store is genuinely this large)");
            return None;
        }
        if !outcome.status.success() {
            let stderr = String::from_utf8_lossy(&outcome.stderr);
            eprintln!("warning: plugin `{exe}` {op} failed: {}", stderr.trim());
            return None;
        }
        let stdout = String::from_utf8_lossy(&outcome.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return None;
        }
        match serde_json::from_str::<T>(trimmed) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("warning: plugin `{exe}` {op} returned invalid JSON: {e}");
                None
            }
        }
    }
}
