//! `.balls/` symlink materialization for the balls-owned state repo,
//! split out of `state_repo.rs` to keep it under the 300-line cap.
//! `state_repo` re-exports these so callers (`ensure`, `remaster`'s
//! detach path) keep their `crate::state_repo::…` paths.

use crate::error::{BallError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Materialize a `.balls/` symlink at `link` pointing to `target` (a
/// path relative to `<root>/.balls/`). Idempotent: a symlink already at
/// the wanted target is a no-op, a stale one is repointed. When a
/// *real* (non-symlink) entry occupies the path, `on_real` decides — it
/// runs for its side effects (clear the entry, or fail) and returns
/// whether the symlink should still be created; `false` leaves the real
/// entry in place, an `Err` aborts. Taking `on_real` as `&dyn Fn`
/// rather than a generic keeps the helper a single monomorphization, so
/// every branch's coverage is attributed to one instance.
pub(crate) fn materialize_symlink(
    link: &Path,
    target: &str,
    on_real: &dyn Fn(&Path) -> Result<bool>,
) -> Result<()> {
    let want = PathBuf::from(target);
    if link.is_symlink() {
        if fs::read_link(link).ok().as_deref() == Some(want.as_path()) {
            return Ok(());
        }
        fs::remove_file(link)?;
    } else if link.exists() && !on_real(link)? {
        return Ok(());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&want, link)?;
    }
    #[cfg(not(unix))]
    {
        return Err(BallError::Other(
            "symlink-mode bl init requires a POSIX filesystem; use stealth mode".into(),
        ));
    }
    Ok(())
}

/// Materialize `.balls/plugins` as a symlink to `target` (relative to
/// `<root>/.balls/`) so per-plugin config files resolve through the
/// project root in federated mode (bl-1098). A real `.balls/plugins/`
/// is removed if `.gitkeep`-only, refused if it carries config files
/// (the migration is `bl remaster`'s job).
pub(crate) fn ensure_plugins_symlink(root: &Path, target: &str) -> Result<()> {
    materialize_symlink(&root.join(".balls/plugins"), target, &|link| {
        drop_placeholder_plugins_dir(link)?;
        Ok(true)
    })
}

/// Materialize `.balls/config.json` as a symlink to `target` (the
/// hub's canonical) — bl-82a4. A *real* `.balls/config.json` is left
/// untouched (standalone, or a legacy federated repo `bl remaster`
/// migrates). The case this materializes is the fresh clone — no
/// canonical, only the committed `.balls/master.json`.
pub(crate) fn ensure_config_symlink(root: &Path, target: &str) -> Result<()> {
    materialize_symlink(&root.join(".balls/config.json"), target, &|_| Ok(false))
}

fn drop_placeholder_plugins_dir(dir: &Path) -> Result<()> {
    let real: Vec<String> = fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n != ".gitkeep")
        .collect();
    if !real.is_empty() {
        return Err(BallError::Other(format!(
            "`.balls/plugins/` contains {real:?}; under master_url the hub is \
             authoritative (bl-a7d9). Move these into the hub's \
             `.balls/plugins/` and remove `.balls/plugins/` here, then retry."
        )));
    }
    fs::remove_dir_all(dir)?;
    Ok(())
}

/// Inverse of `ensure_plugins_symlink`: replace the symlink with a real
/// directory carrying the hub's plugin files at detach time, so the
/// new-standalone repo keeps its plugin config instead of losing it.
/// Idempotent when already a real dir.
pub(crate) fn restore_plugins_dir(root: &Path, state_repo_plugins: &Path) -> Result<()> {
    let link = root.join(".balls/plugins");
    if link.is_symlink() {
        fs::remove_file(&link)?;
    } else if link.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(&link)?;
    if let Ok(rd) = fs::read_dir(state_repo_plugins) {
        for entry in rd.flatten() {
            let from = entry.path();
            if from.is_file() {
                fs::copy(&from, link.join(entry.file_name()))?;
            }
        }
    }
    let keep = link.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, "")?;
    }
    Ok(())
}
