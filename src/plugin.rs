use crate::config::{Config, PluginEntry};
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

    pub fn push(&self, task: &Task) -> Result<()> {
        if !self.is_available() {
            eprintln!(
                "warning: plugin `{}` not found on PATH, skipping push",
                self.executable
            );
            return Ok(());
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
        }
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        if !self.is_available() {
            eprintln!(
                "warning: plugin `{}` not found on PATH, skipping sync",
                self.executable
            );
            return Ok(());
        }
        let out = Command::new(&self.executable)
            .arg("sync")
            .arg("--config")
            .arg(&self.config_path)
            .arg("--auth-dir")
            .arg(&self.auth_dir)
            .output()?;
        if !out.status.success() {
            eprintln!(
                "warning: plugin `{}` sync failed: {}",
                self.executable,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
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

pub fn run_plugin_push(store: &Store, task: &Task) -> Result<()> {
    let cfg = store.load_config()?;
    for (name, entry) in active_plugins(&cfg) {
        if entry.sync_on_change {
            let plugin = Plugin::resolve(store, name, entry);
            let _ = plugin.push(task);
        }
    }
    Ok(())
}

pub fn run_plugin_sync(store: &Store) -> Result<()> {
    let cfg = store.load_config()?;
    for (name, entry) in active_plugins(&cfg) {
        let plugin = Plugin::resolve(store, name, entry);
        let _ = plugin.sync();
    }
    Ok(())
}

fn active_plugins(cfg: &Config) -> impl Iterator<Item = (&String, &PluginEntry)> {
    cfg.plugins.iter().filter(|(_, e)| e.enabled)
}
