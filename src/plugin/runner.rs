use super::types::{PushResponse, SyncReport};
use crate::config::PluginEntry;
use crate::error::Result;
use crate::store::Store;
use crate::task::Task;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct Plugin {
    pub executable: String,
    pub config_path: PathBuf,
    pub auth_dir: PathBuf,
}

impl Plugin {
    pub fn resolve(store: &Store, name: &str, entry: &PluginEntry) -> Self {
        let executable = format!("balls-plugin-{}", name);
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
        let result = Command::new(&self.executable)
            .arg("auth-check")
            .arg("--auth-dir")
            .arg(&self.auth_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();
        match result {
            Ok(out) => {
                if out.status.success() {
                    true
                } else {
                    eprintln!(
                        "warning: {} auth expired. Run `{} auth-setup --auth-dir {}` to re-authenticate.",
                        self.executable,
                        self.executable,
                        self.auth_dir.display()
                    );
                    false
                }
            }
            Err(_) => false,
        }
    }

    /// Run the plugin's push command. Returns the plugin's response (to be
    /// written into task.external) or None if the plugin failed/was unavailable.
    pub fn push(&self, task: &Task) -> Result<Option<PushResponse>> {
        if !self.is_available() {
            eprintln!(
                "warning: plugin `{}` not found on PATH, skipping push",
                self.executable
            );
            return Ok(None);
        }
        let mut child = Command::new(&self.executable)
            .arg("push")
            .arg("--task")
            .arg(&task.id)
            .arg("--config")
            .arg(&self.config_path)
            .arg("--auth-dir")
            .arg(&self.auth_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() {
            let json = serde_json::to_string(task)?;
            let _ = stdin.write_all(json.as_bytes());
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            eprintln!(
                "warning: plugin `{}` push failed: {}",
                self.executable,
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return Ok(None);
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        match serde_json::from_str::<PushResponse>(trimmed) {
            Ok(result) => Ok(Some(result)),
            Err(e) => {
                eprintln!(
                    "warning: plugin `{}` push returned invalid JSON: {}",
                    self.executable, e
                );
                Ok(None)
            }
        }
    }

    /// Run the plugin's sync command. Sends all local tasks on stdin.
    /// Returns a SyncReport or None if the plugin failed.
    pub fn sync(
        &self,
        tasks: &[Task],
        filter: Option<&str>,
    ) -> Result<Option<SyncReport>> {
        if !self.is_available() {
            eprintln!(
                "warning: plugin `{}` not found on PATH, skipping sync",
                self.executable
            );
            return Ok(None);
        }
        let mut cmd = Command::new(&self.executable);
        cmd.arg("sync")
            .arg("--config")
            .arg(&self.config_path)
            .arg("--auth-dir")
            .arg(&self.auth_dir);
        if let Some(task_id) = filter {
            cmd.arg("--task").arg(task_id);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() {
            let json = serde_json::to_string(tasks)?;
            let _ = stdin.write_all(json.as_bytes());
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            eprintln!(
                "warning: plugin `{}` sync failed: {}",
                self.executable,
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return Ok(None);
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        match serde_json::from_str::<SyncReport>(trimmed) {
            Ok(report) => Ok(Some(report)),
            Err(e) => {
                eprintln!(
                    "warning: plugin `{}` sync returned invalid JSON: {}",
                    self.executable, e
                );
                Ok(None)
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
