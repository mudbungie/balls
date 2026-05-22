//! `.balls/plugins` and `.balls/project.json` symlink materialization
//! for the state checkout, split out of `state_repo.rs` to keep it
//! under the 300-line cap. `state_repo` re-exports both `ensure_*`
//! helpers so its callers keep their `crate::state_repo::…` paths.

use crate::error::{BallError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Materialize `.balls/plugins` as a symlink to `target` (a path
/// relative to `<root>/.balls/`) so plugin config resolves through the
/// state checkout. Idempotent; repoints a stale symlink. A real
/// `.balls/plugins/` — a legacy standalone repo pre-migration — has
/// its config files absorbed into the state checkout first, then the
/// directory is replaced by the symlink.
pub(crate) fn ensure_plugins_symlink(root: &Path, target: &str) -> Result<()> {
    let link = root.join(".balls/plugins");
    let want = PathBuf::from(target);
    if link.is_symlink() {
        if fs::read_link(&link).ok().as_deref() == Some(want.as_path()) {
            return Ok(());
        }
        fs::remove_file(&link)?;
    } else if link.is_dir() {
        absorb_plugins_dir(&link, &root.join(".balls").join(&want))?;
    }
    std::os::unix::fs::symlink(&want, &link)?;
    Ok(())
}

/// Materialize `.balls/project.json` as a symlink to `target` (a path
/// relative to `<root>/.balls/`). The project config is a single file
/// on the tracker branch; the symlink lets a workspace read it through
/// `.balls/project.json` (SPEC §7). Idempotent; repoints a stale
/// symlink. A real (non-symlink) file at the path is refused — it is
/// ambiguous with hand-placed config the migration must not shadow.
pub(crate) fn ensure_project_json_symlink(root: &Path, target: &str) -> Result<()> {
    let link = root.join(".balls/project.json");
    let want = PathBuf::from(target);
    if link.is_symlink() {
        if fs::read_link(&link).ok().as_deref() == Some(want.as_path()) {
            return Ok(());
        }
        fs::remove_file(&link)?;
    } else if link.exists() {
        return Err(BallError::Other(format!(
            "unexpected non-symlink at {}; remove it and re-run `bl init`",
            link.display()
        )));
    }
    std::os::unix::fs::symlink(&want, &link)?;
    Ok(())
}

/// Move a legacy real `.balls/plugins/`'s config files into the state
/// checkout's plugins dir (`dest`), then remove the source directory.
/// `.gitkeep` is dropped — `dest` carries its own. Both sit under the
/// repo root, so a plain `rename` always succeeds.
fn absorb_plugins_dir(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)?.flatten() {
        let name = entry.file_name();
        if entry.path().is_file() && name.to_string_lossy() != ".gitkeep" {
            fs::rename(entry.path(), dest.join(&name))?;
        }
    }
    fs::remove_dir_all(src)?;
    Ok(())
}
