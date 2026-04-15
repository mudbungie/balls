use super::diag::prepare_diag_pipe;
use super::limits::{self, run_with_limits, PluginOutcome};
use super::types::{PluginDiagnostic, PushResponse, SyncReport};
use crate::config::PluginEntry;
use crate::error::Result;
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
        let Ok(outcome) = self.spawn_and_run("auth-check", None, &[]) else {
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
        let outcome = self.spawn_and_run("push", Some(&task.id), json.as_bytes())?;
        Ok(self.parse_outcome::<PushResponse>("push", outcome))
    }

    /// Run the plugin's sync command. Sends all local tasks on stdin.
    pub fn sync(
        &self,
        tasks: &[Task],
        filter: Option<&str>,
    ) -> Result<Option<SyncReport>> {
        let json = serde_json::to_string(tasks)?;
        let outcome = self.spawn_and_run("sync", filter, json.as_bytes())?;
        Ok(self.parse_outcome::<SyncReport>("sync", outcome))
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
        task_filter: Option<&str>,
        stdin_bytes: &[u8],
    ) -> Result<PluginOutcome> {
        let mut cmd = self.command(subcmd, task_filter);
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
    fn command(&self, subcmd: &str, task_filter: Option<&str>) -> Command {
        let mut cmd = Command::new(&self.executable);
        cmd.arg(subcmd);
        if let Some(id) = task_filter {
            cmd.arg("--task").arg(id);
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
