//! Shared post-merge auto-resolution. Both `bl sync` and the
//! claim-time push-retry loop need to take a state-worktree mid-merge
//! and resolve `.balls/tasks/*.json` conflicts via the field-level
//! merge in `resolve.rs`. Notes sidecars are handled by git's union
//! merge driver everywhere except delete/modify, which surfaces here.

use crate::error::{BallError, Result};
use crate::{git, resolve};
use std::fs;
use std::path::Path;

/// Walk the conflicted-files list in `dir` and resolve each one.
/// Task JSON files go through `resolve::resolve_conflict`; notes
/// sidecars are accepted as-is (whichever side survived the
/// delete/modify merge); anything else fails loudly.
pub fn auto_resolve_task_conflicts(dir: &Path) -> Result<()> {
    for path in git::git_list_conflicted_files(dir)? {
        let rel = path.strip_prefix(dir).unwrap_or(&path);
        let rel_str = rel.to_string_lossy();
        let rel_p = Path::new(&*rel_str).to_path_buf();
        let is_task = rel_str.starts_with(".balls/tasks/") && rel_str.ends_with(".json");
        let is_notes = rel_str.ends_with(".notes.jsonl");
        if !is_task && !is_notes {
            return Err(BallError::Conflict(format!(
                "unhandled conflict in {}",
                path.display()
            )));
        }
        if is_notes {
            git::git_add(dir, &[rel_p.as_path()])?;
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let (ours, theirs) = resolve::parse_conflict_markers(&content)?;
        let merged = resolve::resolve_conflict(&ours, &theirs);
        merged.save(&path)?;
        git::git_add(dir, &[rel_p.as_path()])?;
    }
    Ok(())
}
