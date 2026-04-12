use crate::error::{BallError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sync_on_change: bool,
    pub config_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub id_length: usize,
    pub stale_threshold_seconds: u64,
    #[serde(default = "default_true")]
    pub auto_fetch_on_ready: bool,
    pub worktree_dir: String,
    /// Override tasks directory. When set to an absolute path outside the repo,
    /// task files are not git-tracked (stealth mode).
    #[serde(default)]
    pub tasks_dir: Option<String>,
    #[serde(default)]
    pub protected_main: bool,
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginEntry>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: 1,
            id_length: 4,
            stale_threshold_seconds: 60,
            auto_fetch_on_ready: true,
            tasks_dir: None,
            worktree_dir: ".balls-worktrees".to_string(),
            protected_main: false,
            plugins: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BallError::NotInitialized,
            _ => BallError::Io(e),
        })?;
        let c: Config = serde_json::from_str(&s)?;
        Ok(c)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, s + "\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested/config.json");
        let cfg = Config::default();
        cfg.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.id_length, 4);
        assert!(loaded.auto_fetch_on_ready);
        assert!(!loaded.protected_main);
        assert!(loaded.plugins.is_empty());
    }

    #[test]
    fn load_missing_returns_not_initialized() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.json");
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, BallError::NotInitialized));
    }

    #[test]
    fn load_bad_json_returns_json_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, BallError::Json(_)));
    }

    #[test]
    fn default_true_fills_in_missing_field() {
        // Omit auto_fetch_on_ready — serde default must be true
        let s = r#"{
            "version": 1,
            "id_length": 4,
            "stale_threshold_seconds": 60,
            "worktree_dir": ".balls-worktrees"
        }"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("c.json");
        std::fs::write(&path, s).unwrap();
        let cfg = Config::load(&path).unwrap();
        assert!(cfg.auto_fetch_on_ready);
    }

    #[test]
    fn load_non_notfound_io_error() {
        // A directory at the config path yields an IO error that's not NotFound.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub");
        std::fs::create_dir_all(&path).unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, BallError::Io(_)));
    }

    #[test]
    fn plugin_entry_serde() {
        let mut cfg = Config::default();
        cfg.plugins.insert(
            "jira".to_string(),
            PluginEntry {
                enabled: true,
                sync_on_change: true,
                config_file: ".balls/plugins/jira.json".into(),
            },
        );
        let s = serde_json::to_string(&cfg).unwrap();
        assert!(s.contains("jira"));
        let back: Config = serde_json::from_str(&s).unwrap();
        assert_eq!(back.plugins.len(), 1);
    }
}
