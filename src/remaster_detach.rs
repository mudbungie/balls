//! `bl remaster --detach` — sever the tracker link and return the
//! workspace to the implicit default (the code repo's own `origin`).
//!
//! There is one mechanism and one layout, so detach is not a
//! transplant — it is an address edit (the caller clears `state_url`)
//! plus a re-root of the existing `.balls/state-repo`. It is
//! offline-capable: a workspace whose explicit tracker never
//! materialized is detached purely by the config edit, and a warm
//! checkout is re-rooted with no network round-trip.

use crate::error::Result;
use crate::{git, git_state};
use std::path::Path;

/// Sever shared history: re-root the state branch in `.balls/state-repo`
/// as a fresh local orphan carrying its current tasks, and re-point
/// the checkout's `origin` at the code repo's own remote — the
/// implicit default the address now resolves to. A no-op when the
/// state checkout was never materialized (a cold detach is purely the
/// `state_url` edit the caller already made). The branch name is read
/// from the checkout's own HEAD — the authoritative materialized
/// branch — so a non-default `state_branch` is re-rooted in place.
pub fn detach(root: &Path) -> Result<()> {
    let sd = root.join(crate::state_repo::STATE_REPO_REL);
    if !sd.join(".git").exists() {
        return Ok(());
    }
    let branch = git::git_current_branch(&sd)?;
    git_state::reroot_orphan(&sd, &branch, "balls: remaster --detach (standalone)")?;
    match git_state::remote_url(root, "origin") {
        Some(url) => git_state::set_remote(&sd, "origin", &url)?,
        None => git_state::remove_remote(&sd, "origin"),
    }
    Ok(())
}
