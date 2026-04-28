use super::diag::prepare_diag_pipe;
use super::limits::{self, run_with_limits, PluginOutcome};
use super::native_types::{DescribeResponse, ProposeResponse};
use super::types::{PluginDiagnostic, PushResponse, SyncReport};
use crate::config::PluginEntry;
use crate::error::Result;
use crate::participant::Event;
use crate::store::Store;
use crate::task::Task;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub struct Plugin {
    pub executable: String,
    pub config_path: PathBuf,
    pub auth_dir: PathBuf,
}

impl Plugin {
    pub fn resolve(store: &Store, name: &str, entry: &PluginEntry) -> Self {
        let executable = format!("balls-plugin-{name}");
        let config_path = store.root.join(&entry.config_file);
        let auth_dir = store.local_plugins_dir().join(name);
        Plugin {
            executable,
            config_path,
            auth_dir,
        }
    }

    fn is_available(&self) -> bool {
        which(&self.executable).is_some()
    }

    /// Check if the plugin's auth is valid. Returns true if auth is good,
    /// false if expired/missing. Plugin diagnostics (if any) are
    /// rendered before the warning, so a plugin that explains *why* auth
    /// is missing via the diagnostics channel gets its message surfaced.
    pub fn auth_check(&self) -> bool {
        if !self.is_available() {
            return false;
        }
        let Ok(outcome) = self.spawn_and_run("auth-check", &[], &[]) else {
            return false;
        };
        self.render_diagnostics("auth-check", &outcome.diagnostics);
        if outcome.status.success() {
            true
        } else {
            eprintln!(
                "warning: {} auth expired. Run `{} auth-setup --config {} --auth-dir {}` to re-authenticate.",
                self.executable,
                self.executable,
                self.config_path.display(),
                self.auth_dir.display()
            );
            false
        }
    }

    /// Run the plugin's push command. Returns the plugin's response
    /// (stored into task.external) or None if the plugin failed,
    /// timed out, or exceeded the output cap.
    pub fn push(&self, task: &Task) -> Result<Option<PushResponse>> {
        let json = serde_json::to_string(task)?;
        let outcome =
            self.spawn_and_run("push", &[("--task", &task.id)], json.as_bytes())?;
        Ok(self.parse_outcome::<PushResponse>("push", outcome))
    }

    /// Run the plugin's sync command. Sends all local tasks on stdin.
    pub fn sync(
        &self,
        tasks: &[Task],
        filter: Option<&str>,
    ) -> Result<Option<SyncReport>> {
        let json = serde_json::to_string(tasks)?;
        let extra: &[(&str, &str)] = match filter {
            Some(id) => &[("--task", id)],
            None => &[],
        };
        let outcome = self.spawn_and_run("sync", extra, json.as_bytes())?;
        Ok(self.parse_outcome::<SyncReport>("sync", outcome))
    }

    /// Native protocol §5.2 (bl-8b71): plugin self-describes its
    /// projection and event subscriptions. Returns `None` when the
    /// plugin doesn't ship the subcommand (silent fall-through to the
    /// legacy shim per SPEC §12) or when its output is unparseable.
    pub fn describe(&self) -> Result<Option<DescribeResponse>> {
        if !self.is_available() {
            return Ok(None);
        }
        let outcome = self.spawn_and_run("describe", &[], &[])?;
        Ok(self.parse_outcome::<DescribeResponse>("describe", outcome))
    }

    /// Native protocol §5.3: plugin proposes its post-event projection
    /// for one task. Returns `None` for any wire failure — the caller
    /// converts that into `AttemptClass::Other`. A successful
    /// `ProposeResponse` may carry either an `ok` or `conflict`
    /// branch; both are handed back unchanged.
    pub fn propose(&self, event: Event, task: &Task) -> Result<Option<ProposeResponse>> {
        let json = serde_json::to_string(task)?;
        let event_name = event_subcommand_arg(event);
        let outcome = self.spawn_and_run(
            "propose",
            &[("--event", event_name)],
            json.as_bytes(),
        )?;
        Ok(self.parse_outcome::<ProposeResponse>("propose", outcome))
    }

    /// Spawn the plugin with stdin/stdout/stderr piped and a
    /// diagnostics channel on FD 3 (advertised via BALLS_DIAG_FD).
    /// Pipes the given bytes on stdin, runs under the usual limits,
    /// and returns the outcome including any diagnostics the plugin
    /// wrote. The child becomes the sole writer of the diag pipe
    /// after we drop our write end, so the pipe EOFs on exit.
    fn spawn_and_run(
        &self,
        subcmd: &str,
        extra_flags: &[(&str, &str)],
        stdin_bytes: &[u8],
    ) -> Result<PluginOutcome> {
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
    fn render_diagnostics(&self, op: &str, diag_bytes: &[u8]) {
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
    fn command(&self, subcmd: &str, extra_flags: &[(&str, &str)]) -> Command {
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

    fn parse_outcome<T: for<'de> serde::Deserialize<'de>>(
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
            let cap = limits::max_stream_bytes();
            eprintln!("warning: plugin `{exe}` {op} exceeded {cap} bytes of stdout, discarding");
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

/// Map an `Event` to the lowercase wire name a native plugin
/// receives via `--event`. Stable identifier — same as the serde
/// rename used on the JSON-side `Event` enum.
fn event_subcommand_arg(event: Event) -> &'static str {
    match event {
        Event::Claim => "claim",
        Event::Review => "review",
        Event::Close => "close",
        Event::Update => "update",
        Event::Sync => "sync",
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for p in std::env::split_paths(&paths) {
        let candidate = p.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_subcommand_arg_covers_every_event() {
        // Pin the wire name for every Event variant so a rename
        // changes both this test and the JSON serde rename together.
        assert_eq!(event_subcommand_arg(Event::Claim), "claim");
        assert_eq!(event_subcommand_arg(Event::Review), "review");
        assert_eq!(event_subcommand_arg(Event::Close), "close");
        assert_eq!(event_subcommand_arg(Event::Update), "update");
        assert_eq!(event_subcommand_arg(Event::Sync), "sync");
    }
}
