use super::limits::{self, run_with_limits, PluginOutcome};
use super::types::{PushResponse, SyncReport};
use crate::config::PluginEntry;
use crate::error::Result;
use crate::store::Store;
use crate::task::Task;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
    /// false if expired/missing. Prints a warning on auth failure.
    pub fn auth_check(&self) -> bool {
        if !self.is_available() {
            return false;
        }
        let result = self
            .command("auth-check", None)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();
        match result {
            Ok(out) => {
                if out.status.success() {
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
            Err(_) => false,
        }
    }

    /// Run the plugin's push command. Returns the plugin's response
    /// (stored into task.external) or None if the plugin failed,
    /// timed out, or exceeded the output cap.
    pub fn push(&self, task: &Task) -> Result<Option<PushResponse>> {
        let child = self
            .command("push", Some(&task.id))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let json = serde_json::to_string(task)?;
        let outcome = run_with_limits(child, json.as_bytes())?;
        Ok(self.parse_outcome::<PushResponse>("push", outcome))
    }

    /// Run the plugin's sync command. Sends all local tasks on stdin.
    pub fn sync(
        &self,
        tasks: &[Task],
        filter: Option<&str>,
    ) -> Result<Option<SyncReport>> {
        let child = self
            .command("sync", filter)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let json = serde_json::to_string(tasks)?;
        let outcome = run_with_limits(child, json.as_bytes())?;
        Ok(self.parse_outcome::<SyncReport>("sync", outcome))
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
