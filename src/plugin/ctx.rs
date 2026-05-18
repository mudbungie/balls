//! SPEC §5.1 EventCtx side channel.
//!
//! A per-invocation, mode-0600 temp file holding the `EventCtx` JSON,
//! handed to a context-aware native plugin via `--ctx-file <path>`
//! and RAII-removed after the call. `--ctx-file` (a SPEC-sanctioned
//! alternative to a dedicated FD) is chosen deliberately: `tempfile`
//! is a dev-only dependency, and the dedicated-FD precedent
//! (`diag.rs`) runs child→parent — a parent→child context stream
//! would be new pipe + writer-thread plumbing for no gain over a
//! file. A plugin that did not set `wants_context` is never passed
//! `--ctx-file` and receives byte-identical input to today.

use crate::error::{BallError, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// An owned temp file that deletes itself on drop. Created with
/// `create_new` (O_EXCL) at mode 0600 so it never clobbers and is not
/// world-readable.
pub struct CtxFile {
    path: PathBuf,
    path_str: String,
}

impl CtxFile {
    pub fn new(contents: &str) -> Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "balls-eventctx-{}-{}.json",
            std::process::id(),
            nanos
        ));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(BallError::Io)?;
        f.write_all(contents.as_bytes()).map_err(BallError::Io)?;
        let path_str = path.to_string_lossy().into_owned();
        Ok(Self { path, path_str })
    }

    pub fn path_str(&self) -> &str {
        &self.path_str
    }
}

impl Drop for CtxFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn writes_at_0600_then_removes_on_drop() {
        let p;
        {
            let cf = CtxFile::new(r#"{"schema_version":1}"#).unwrap();
            p = cf.path_str().to_string();
            assert_eq!(
                std::fs::read_to_string(&p).unwrap(),
                r#"{"schema_version":1}"#
            );
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "ctx file must not be world-readable");
        }
        assert!(
            !std::path::Path::new(&p).exists(),
            "the file must be gone once the CtxFile is dropped"
        );
    }
}
