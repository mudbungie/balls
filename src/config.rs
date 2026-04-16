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
            version: CONFIG_SCHEMA_VERSION,
            id_length: 4,
            stale_threshold_seconds: 60,
            auto_fetch_on_ready: true,
            worktree_dir: ".balls-worktrees".to_string(),
            protected_main: false,
            plugins: BTreeMap::new(),
        }
    }
}

pub const ID_LENGTH_MIN: usize = 4;
pub const ID_LENGTH_MAX: usize = 32;

/// Current on-disk schema version for `.balls/config.json`. Bump this
/// when a config change requires migration logic. Older clients
/// reading a config written with a higher version refuse to load
/// with a clear "your bl is too old" error rather than silently
/// losing fields. Lower-or-equal versions load normally because the
/// struct definition is backward-compatible by design (new fields
/// carry serde defaults).
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BallError::NotInitialized,
            _ => BallError::Io(e),
        })?;
        let mut c: Config = serde_json::from_str(&s)?;
        c.sanitize();
        c.validate()?;
        Ok(c)
    }

    /// Clamp `id_length` into the supported range, warning on clamp.
    /// id_length = 0 would otherwise infinite-loop id generation; very large
    /// values waste hex space without colliding any less.
    fn sanitize(&mut self) {
        if !(ID_LENGTH_MIN..=ID_LENGTH_MAX).contains(&self.id_length) {
            let original = self.id_length;
            self.id_length = self.id_length.clamp(ID_LENGTH_MIN, ID_LENGTH_MAX);
            eprintln!(
                "warning: id_length {} out of range [{}, {}]; clamped to {}",
                original, ID_LENGTH_MIN, ID_LENGTH_MAX, self.id_length
            );
        }
    }

    /// Reject `worktree_dir` values that would escape the repo root,
    /// and refuse configs written with a schema version newer than
    /// this binary understands.
    fn validate(&self) -> Result<()> {
        if self.version > CONFIG_SCHEMA_VERSION {
            return Err(BallError::Other(format!(
                "config schema version {} is newer than this bl (supports up to {}); \
                 upgrade bl to read this repo's config",
                self.version, CONFIG_SCHEMA_VERSION
            )));
        }
        if self.worktree_dir.starts_with('/') || self.worktree_dir.contains("..") {
            return Err(BallError::Other(format!(
                "invalid config: worktree_dir {:?} must be a relative path with no '..' segments",
                self.worktree_dir
            )));
        }
        Ok(())
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

    fn write_cfg(dir: &TempDir, body: &str) -> std::path::PathBuf {
        let path = dir.path().join("c.json");
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn id_length_clamped_low() {
        let dir = TempDir::new().unwrap();
        let p = write_cfg(
            &dir,
            r#"{"version":1,"id_length":0,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}"#,
        );
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg.id_length, ID_LENGTH_MIN);
    }

    #[test]
    fn id_length_clamped_high() {
        let dir = TempDir::new().unwrap();
        let p = write_cfg(
            &dir,
            r#"{"version":1,"id_length":99,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}"#,
        );
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg.id_length, ID_LENGTH_MAX);
    }

    #[test]
    fn worktree_dir_rejects_absolute_path() {
        let dir = TempDir::new().unwrap();
        let p = write_cfg(
            &dir,
            r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":"/tmp/evil"}"#,
        );
        let err = Config::load(&p).unwrap_err();
        assert!(matches!(err, BallError::Other(ref s) if s.contains("worktree_dir")));
    }

    #[test]
    fn worktree_dir_rejects_parent_segment() {
        let dir = TempDir::new().unwrap();
        let p = write_cfg(
            &dir,
            r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":"../escape"}"#,
        );
        let err = Config::load(&p).unwrap_err();
        assert!(matches!(err, BallError::Other(ref s) if s.contains("worktree_dir")));
    }

    #[test]
    fn load_rejects_future_schema_version() {
        let dir = TempDir::new().unwrap();
        let future = CONFIG_SCHEMA_VERSION + 1;
        let p = write_cfg(
            &dir,
            &format!(
                r#"{{"version":{future},"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}}"#
            ),
        );
        let err = Config::load(&p).unwrap_err();
        assert!(
            matches!(err, BallError::Other(ref s) if s.contains("schema version") && s.contains("upgrade bl")),
            "expected schema-version error, got: {err:?}",
        );
    }

    #[test]
    fn load_accepts_current_schema_version() {
        let dir = TempDir::new().unwrap();
        let p = write_cfg(
            &dir,
            &format!(
                r#"{{"version":{CONFIG_SCHEMA_VERSION},"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}}"#
            ),
        );
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg.version, CONFIG_SCHEMA_VERSION);
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
