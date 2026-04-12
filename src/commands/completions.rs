//! Shell completion installation to standard XDG data directories.

use balls::error::Result;
use clap_complete::Shell;
use std::fs;
use std::path::{Path, PathBuf};

/// Standard install paths for `bl` completion files, relative to `$HOME`.
fn targets(home: &Path) -> [(Shell, PathBuf); 3] {
    [
        (
            Shell::Bash,
            home.join(".local/share/bash-completion/completions/bl"),
        ),
        (Shell::Zsh, home.join(".local/share/zsh/site-functions/_bl")),
        (
            Shell::Fish,
            home.join(".local/share/fish/vendor_completions.d/bl.fish"),
        ),
    ]
}

/// Write bash, zsh, and fish completion files under `home`. Returns the
/// list of paths written, in install order.
pub fn install_completions(cmd: &mut clap::Command, home: &Path) -> Result<Vec<PathBuf>> {
    let mut written = Vec::with_capacity(3);
    for (shell, path) in targets(home) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut buf: Vec<u8> = Vec::new();
        clap_complete::generate(shell, cmd, "bl", &mut buf);
        fs::write(&path, &buf)?;
        written.push(path);
    }
    Ok(written)
}

/// Remove the three completion files written by `install_completions`.
/// Missing files are ignored. Returns the paths that were removed.
pub fn uninstall_completions(home: &Path) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for (_, path) in targets(home) {
        if path.exists() {
            fs::remove_file(&path)?;
            removed.push(path);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};
    use tempfile::tempdir;

    #[derive(Parser)]
    #[command(name = "bl")]
    struct DummyCli {
        #[arg(long)]
        _flag: bool,
    }

    fn dummy_cmd() -> clap::Command {
        DummyCli::command()
    }

    #[test]
    fn install_writes_three_files_with_content() {
        let tmp = tempdir().unwrap();
        let written = install_completions(&mut dummy_cmd(), tmp.path()).unwrap();
        assert_eq!(written.len(), 3);
        for path in &written {
            assert!(path.exists(), "missing: {}", path.display());
            let bytes = fs::read(path).unwrap();
            assert!(!bytes.is_empty(), "empty: {}", path.display());
        }
        assert!(written[0].ends_with("bash-completion/completions/bl"));
        assert!(written[1].ends_with("zsh/site-functions/_bl"));
        assert!(written[2].ends_with("fish/vendor_completions.d/bl.fish"));
    }

    #[test]
    fn uninstall_removes_only_existing_files() {
        let tmp = tempdir().unwrap();
        // Nothing installed yet — uninstall is a no-op.
        assert!(uninstall_completions(tmp.path()).unwrap().is_empty());
        // Install, then uninstall — all three come back.
        install_completions(&mut dummy_cmd(), tmp.path()).unwrap();
        let removed = uninstall_completions(tmp.path()).unwrap();
        assert_eq!(removed.len(), 3);
        for p in &removed {
            assert!(!p.exists());
        }
    }

}
