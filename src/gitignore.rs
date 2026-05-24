//! Clone-checkout `.gitignore` maintenance.
//!
//! `bl init` keeps the clone's `.gitignore` carrying every
//! balls-internal runtime path so a careless `git add -A` can't bake
//! balls's own filesystem state into the shared repo. *Which* paths
//! those are is the `runtime_paths` table's job (the single source of
//! truth, bl-228d); this module is only the file mechanic — append
//! the missing lines.

use crate::error::Result;
use std::fs;
use std::path::Path;

/// Append any runtime path missing from `<root>/.gitignore`, one per
/// line. The path set comes from `runtime_paths::gitignore_paths`,
/// which drops the stealth-absent state-checkout paths. A non-existent
/// file is treated as empty and created. Idempotent.
pub fn ensure_main_gitignore(root: &Path, is_stealth: bool) -> Result<()> {
    let entries = crate::runtime_paths::gitignore_paths(is_stealth);
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

#[cfg(test)]
#[path = "gitignore_tests.rs"]
mod tests;
