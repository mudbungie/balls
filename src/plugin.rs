use crate::config::{Config, PluginEntry};
use crate::error::Result;
use crate::store::{task_lock, Store};
use crate::task::Task;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// ---------------------------------------------------------------------------
// Protocol types
// ---------------------------------------------------------------------------

/// What a plugin returns on stdout after a successful `push`.
/// Core stores this verbatim into `task.external.{plugin_name}`.
#[derive(Debug, Clone, Deserialize)]
pub struct PushResponse(pub serde_json::Map<String, Value>);

/// Full sync report returned by the plugin on stdout after `sync`.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncReport {
    #[serde(default)]
    pub created: Vec<SyncCreate>,
    #[serde(default)]
    pub updated: Vec<SyncUpdate>,
    #[serde(default)]
    pub deleted: Vec<SyncDelete>,
}

/// A new task to create locally, reported by plugin sync.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncCreate {
    pub title: String,
    #[serde(rename = "type", default = "default_task_type")]
    pub task_type: String,
    #[serde(default = "default_priority")]
    pub priority: u8,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub external: serde_json::Map<String, Value>,
}

/// Fields to update on an existing local task.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncUpdate {
    pub task_id: String,
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    #[serde(default)]
    pub external: serde_json::Map<String, Value>,
    pub add_note: Option<String>,
}

/// A local task to mark as deferred.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncDelete {
    pub task_id: String,
    #[serde(default)]
    pub reason: String,
}

fn default_task_type() -> String {
    "task".into()
}
fn default_priority() -> u8 {
    3
}
fn default_status() -> String {
    "open".into()
}

// ---------------------------------------------------------------------------
// Plugin struct and methods
// ---------------------------------------------------------------------------

pub struct Plugin {
    pub executable: String,
    pub config_path: PathBuf,
    pub auth_dir: PathBuf,
}

impl Plugin {
    pub fn resolve(store: &Store, name: &str, entry: &PluginEntry) -> Self {
        let executable = format!("ball-plugin-{}", name);
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

// ---------------------------------------------------------------------------
// Dispatch functions
// ---------------------------------------------------------------------------

/// Run plugin push for all active plugins. Returns a map of
/// plugin_name -> PushResponse for plugins that returned data.
pub fn run_plugin_push(store: &Store, task: &Task) -> Result<BTreeMap<String, PushResponse>> {
    let cfg = store.load_config()?;
    let mut results = BTreeMap::new();
    for (name, entry) in active_plugins(&cfg) {
        if entry.sync_on_change {
            let plugin = Plugin::resolve(store, name, entry);
            if !plugin.auth_check() {
                continue;
            }
            if let Ok(Some(result)) = plugin.push(task) {
                results.insert(name.clone(), result);
            }
        }
    }
    Ok(results)
}

/// Run plugin sync for all active plugins. Returns (plugin_name, SyncReport) pairs.
pub fn run_plugin_sync(
    store: &Store,
    filter: Option<&str>,
) -> Result<Vec<(String, SyncReport)>> {
    let cfg = store.load_config()?;
    let tasks = store.all_tasks()?;
    let mut reports = Vec::new();
    for (name, entry) in active_plugins(&cfg) {
        let plugin = Plugin::resolve(store, name, entry);
        if !plugin.auth_check() {
            continue;
        }
        if let Ok(Some(report)) = plugin.sync(&tasks, filter) {
            reports.push((name.clone(), report));
        }
    }
    Ok(reports)
}

/// Merge push results into task.external and save+commit.
pub fn apply_push_response(
    store: &Store,
    task_id: &str,
    results: &BTreeMap<String, PushResponse>,
) -> Result<()> {
    if results.is_empty() {
        return Ok(());
    }
    let _g = task_lock(store, task_id)?;
    let mut task = store.load_task(task_id)?;
    for (plugin_name, response) in results {
        let ext_value = Value::Object(response.0.clone());
        task.external.insert(plugin_name.clone(), ext_value);
    }
    task.touch();
    store.save_task(&task)?;
    store.commit_task(task_id, &format!("ball: update external for {}", task_id))?;
    Ok(())
}

fn active_plugins(cfg: &Config) -> impl Iterator<Item = (&String, &PluginEntry)> {
    cfg.plugins.iter().filter(|(_, e)| e.enabled)
}
