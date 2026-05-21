//! Main-checkout `.gitignore` maintenance.
//!
//! `bl init` and the `bl remaster` federated flip keep the project's
//! `.gitignore` carrying every balls-internal runtime path so a
//! careless `git add -A` can't bake balls's own filesystem state into
//! the shared repo. *Which* paths those are is the `runtime_paths`
//! table's job (the single source of truth, bl-228d); this module is
//! only the file mechanic — append the missing lines, or, for the
//! `bl remaster --detach` reversal, drop lines that no longer apply.

use crate::error::Result;
use std::fs;
use std::path::Path;

/// Append any runtime path missing from `<root>/.gitignore`, one per
/// line. The path set comes from `runtime_paths::gitignore_paths`,
/// which drops the stealth-absent state-worktree paths and — unless
/// `federated` — the `master_url`-only paths. A non-existent file is
/// treated as empty and created. Idempotent.
pub fn ensure_main_gitignore(root: &Path, is_stealth: bool, federated: bool) -> Result<()> {
    let entries = crate::runtime_paths::gitignore_paths(is_stealth, federated);
    add_entries(root, &entries)
}

fn add_entries(root: &Path, entries: &[&str]) -> Result<()> {
    let path = root.join(".gitignore");
    let mut content = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let mut dirty = false;
    for entry in entries {
        if content.lines().any(|l| l.trim() == *entry) {
            continue;
        }
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(entry);
        content.push('\n');
        dirty = true;
    }
    if dirty {
        fs::write(&path, content)?;
    }
    Ok(())
}

/// `bl remaster --detach`'s reversal of the federated flip: drop the
/// `master_url`-only paths from `<root>/.gitignore` so `.balls/plugins`,
/// a real tracked directory again, comes back out of the ignore list
/// (bl-ebae). Idempotent.
pub fn remove_federated_entries(root: &Path) -> Result<()> {
    remove_entries(root, &crate::runtime_paths::federated_only_paths())
}

/// Drop `entries` from `<root>/.gitignore`. Entries not present, and a
/// missing file, are silently no-ops — idempotent.
fn remove_entries(root: &Path, entries: &[&str]) -> Result<()> {
    let path = root.join(".gitignore");
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path)?;
    let mut kept: Vec<&str> = Vec::new();
    let mut dirty = false;
    for line in content.lines() {
        if entries.contains(&line.trim()) {
            dirty = true;
        } else {
            kept.push(line);
        }
    }
    if dirty {
        let mut out = kept.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        fs::write(&path, out)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "gitignore_tests.rs"]
mod tests;
