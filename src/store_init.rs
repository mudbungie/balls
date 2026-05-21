//! Helpers for `Store::init`: state branch creation, state worktree
//! setup, main-checkout gitignore wiring, and the bare-hub bootstrap.
//! Extracted from `store.rs` to keep that file focused on the Store API.

use crate::config::Config;
use crate::error::{BallError, Result};
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const STATE_BRANCH: &str = "balls/tasks";

/// Relative path (from the repo root) of the state worktree's checkout.
pub(crate) const STATE_WORKTREE_REL: &str = ".balls/worktree";

/// Ensure the orphan state branch exists, is checked out at
/// `.balls/worktree/`, has its schema scaffolding (`.gitattributes`,
/// `.gitkeep`) seeded, and is exposed to the main checkout via a stable
/// `.balls/tasks` symlink.
///
/// `remote` is the resolved `state_remote` (default `origin`). It is
/// the only remote this function touches: tracking, fetch, and the
/// best-effort publishes all target it, so a client repo pointed at a
/// shared task hub adopts the hub's `balls/tasks` instead of forking
/// an unrelated orphan. The code remote is never referenced here.
/// `linked` is true when the committed config explicitly set
/// `state_remote` (vs the defaulted `origin`); it gates the
/// not-yet-joined advisory below.
///
/// Safety invariant (bl-8e8f): init is never destructive to a shared
/// branch. It only ever *tracks* an existing remote branch or
/// *creates* a local orphan and tries a plain (non-force) push. A
/// divergent or non-empty remote `balls/tasks` makes that push a
/// no-op (git rejects the non-fast-forward); init never resets,
/// force-pushes, or overwrites it. Joining a hub whose history you've
/// diverged from is `bl remaster`'s job, an explicit reconcile step.
pub(crate) fn setup_state_branch(root: &Path, remote: &str, linked: bool) -> Result<()> {
    let has_remote = git::git_has_remote(root, remote);
    if !git_state::branch_exists(root, STATE_BRANCH) {
        // Prefer tracking the remote copy: two `bl init`s in separate
        // clones must not each create a fresh orphan, or their histories
        // will be unrelated and sync will refuse to merge them.
        if has_remote {
            let _ = git::git_fetch(root, remote);
        }
        if has_remote && git_state::has_remote_branch(root, remote, STATE_BRANCH) {
            git_state::create_tracking_branch(root, STATE_BRANCH, remote)?;
        } else {
            git_state::create_orphan_branch(root, STATE_BRANCH, "balls state")?;
            // Best-effort publish so a second clone's init discovers the
            // branch on the remote and tracks it instead of creating its
            // own unrelated orphan. Offline? Fine — first sync will push.
            // A non-empty remote rejects this non-force push: that is
            // the non-clobber guarantee, not an error.
            if has_remote {
                let _ = git::git_push(root, remote, STATE_BRANCH);
            }
        }
    }

    let state_wt = root.join(STATE_WORKTREE_REL);
    if !state_wt.join(".git").exists() {
        if let Some(parent) = state_wt.parent() {
            fs::create_dir_all(parent)?;
        }
        // Operator may have removed the checkout dir to fix corruption
        // (the path doctor's legacy-worktree hint names). Clear any
        // dangling registry entry so the re-add isn't blocked.
        let _ = git_state::worktree_prune(root);
        git_state::worktree_add_existing(root, &state_wt, STATE_BRANCH)?;
    }

    seed_state_worktree(&state_wt)?;
    ensure_tasks_symlink(root, "worktree/.balls/tasks")?;

    // Publish the seed commit if we made one above and the branch is
    // freshly-local. Best-effort.
    if has_remote {
        let _ = git::git_push(root, remote, STATE_BRANCH);
    }

    // The committed config declares a `state_remote` this clone can't
    // reach (no git remote of that name): an unaware `bl init` here is
    // safe-but-unlinked — a usable isolated local store, never a
    // destructive surprise. Surface that and the explicit join path so
    // the divergence is a known state, not a silent one.
    if linked && !has_remote {
        eprintln!(
            "note: .balls/config.json sets state_remote `{remote}`, but this \
             clone has no git remote `{remote}`. Created an isolated local \
             task store — your tasks are not shared with the project yet. \
             Add the remote, then run `bl remaster {remote}` to join \
             (non-destructive)."
        );
    }
    Ok(())
}

/// Bootstrap a bare central hub at `hubdir` from `source` (a git URL or
/// path whose `main` already carries a balls-initialized project and
/// whose `balls/tasks` orphan branch is published — README step 1).
/// Mechanizes the by-hand README *Bootstrapping a bare hub from scratch*
/// steps 2–3. Returns the resolved hub root.
///
/// Idempotent and self-healing, exactly like the working-tree
/// `Store::init`: a present bare gitdir is reused (a non-bare `.git`
/// there is refused, never clobbered), scaffolding `create_dir_all`s
/// are no-ops when present, `config.json` is materialized only when
/// missing, and the shared `setup_state_branch` already tolerates a
/// pre-existing worktree/symlink. The working-tree-only work (the
/// `balls: initialize` commit, main `.gitignore`, plugins `.gitkeep`)
/// is *correctly* skipped: a bare root has no checkout to write it to.
pub(crate) fn bootstrap_bare_hub(source: &str, hubdir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(hubdir)?;
    let hubdir = fs::canonicalize(hubdir).unwrap_or_else(|_| hubdir.to_path_buf());
    let gitdir = hubdir.join(".git");

    // Step 2: bare-clone into <hub>/.git. Reuse an existing bare gitdir;
    // refuse to clobber a non-bare one (non-destructive, like bl init).
    if gitdir.exists() {
        if !crate::bare_squash::is_bare_repo(&hubdir).unwrap_or(false) {
            return Err(BallError::Other(format!(
                "{} exists and is not a bare repo; refusing to clobber it",
                gitdir.display()
            )));
        }
    } else {
        git::git_clone_bare(source, &gitdir)?;
    }
    // Wire remote-tracking so origin/* refs stay current for bl sync.
    git::git_config_set(&hubdir, "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*")?;
    let _ = git::git_fetch(&hubdir, "origin");
    git::git_ensure_user(&hubdir)?;

    // Step 3: reconstruct the loose store. Scaffold the per-hub dirs and
    // materialize the main-tracked config.json (no checkout exists to
    // copy it from at a bare root), then reuse the shared state wiring.
    let balls = hubdir.join(".balls");
    for d in ["plugins", "local/claims", "local/lock", "local/plugins"] {
        fs::create_dir_all(balls.join(d))?;
    }
    let config_path = balls.join("config.json");
    if !config_path.exists() {
        let cfg = git_state::show_file(&hubdir, "main", ".balls/config.json")?.ok_or_else(|| {
            BallError::Other(
                "source's `main` has no .balls/config.json — run `bl init` in a \
                 working clone and push first (README bootstrap step 1)"
                    .into(),
            )
        })?;
        fs::write(&config_path, cfg)?;
    }
    let cfg = Config::load(&config_path)?;
    setup_state_branch(&hubdir, cfg.state_remote(), cfg.state_remote.is_some())?;
    Ok(hubdir)
}

/// Seed the state worktree's task directory on first setup: create the
/// `.balls/tasks/` directory, drop in the `.gitattributes` rule that
/// activates git's built-in union merge driver for notes sidecars, and
/// commit anything new so the state branch has a valid HEAD.
fn seed_state_worktree(state_wt: &Path) -> Result<()> {
    let tasks = state_wt.join(".balls/tasks");
    fs::create_dir_all(&tasks)?;

    let attrs = tasks.join(".gitattributes");
    let attrs_line = "*.notes.jsonl merge=union\n";
    let need_attrs = match fs::read_to_string(&attrs) {
        Ok(s) => !s.contains("*.notes.jsonl merge=union"),
        Err(_) => true,
    };
    if need_attrs {
        fs::write(&attrs, attrs_line)?;
    }

    let keep = tasks.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, "")?;
    }

    if git::has_uncommitted_changes(state_wt)? {
        git::git_add_all(state_wt)?;
        git::git_commit(state_wt, "balls: seed state branch")?;
    }
    Ok(())
}

/// Expose the state checkout's task directory to the main checkout via a
/// stable symlink: `<root>/.balls/tasks -> <target>`. `target` is a path
/// relative to `<root>/.balls/` — `worktree/.balls/tasks` in legacy
/// mode, `state-repo/.balls/tasks` in master_url mode. An engineer on
/// main can `ls .balls/tasks/`, `cat`, and `$EDITOR` files through this
/// symlink without any balls-specific knowledge.
///
/// A pre-existing symlink with a different target is repointed (bl-773e):
/// a repo flipped from legacy to master_url via `bl remaster --commit`
/// otherwise keeps a symlink to the deleted `.balls/worktree/`, silently
/// dangling. Matching targets are still no-ops, so the idempotent path
/// is unchanged.
pub(crate) fn ensure_tasks_symlink(root: &Path, target: &str) -> Result<()> {
    let link = root.join(".balls/tasks");
    let want = PathBuf::from(target);
    if link.is_symlink() {
        if fs::read_link(&link).ok().as_deref() == Some(want.as_path()) {
            return Ok(());
        }
        fs::remove_file(&link)?;
    } else if link.exists() {
        // Pre-existing directory or file at the symlink path is ambiguous:
        // refuse to overwrite to avoid clobbering uncommitted tasks.
        return Err(crate::error::BallError::Other(format!(
            "unexpected non-symlink at {}; remove it and re-run `bl init`",
            link.display()
        )));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&want, &link)?;
    }
    #[cfg(not(unix))]
    {
        return Err(crate::error::BallError::Other(
            "symlink-mode bl init requires a POSIX filesystem; use stealth mode".into(),
        ));
    }
    Ok(())
}

/// Commit the init-time files (.gitignore, config.json, plugins .gitkeep)
/// to the main branch. Extracted from Store::init so the no-git path
/// can skip it without duplicating the line-budget in store.rs.
///
/// bl-1098/bl-4432: in master_url mode `.balls/plugins` is a hub-owned
/// symlink, not a project directory — there is no project-owned
/// `.gitkeep` to seed or stage. Read that from `master_url` in config,
/// not by probing the symlink (which couples to `Store::init` order).
///
/// bl-ebae: that federated case also gitignores `.balls/plugins` and
/// drops any standalone-era `.gitkeep` still tracked from a pre-flip
/// `bl init`, so a fresh-cloned federated repo has a clean tree.
pub(crate) fn commit_init(root: &Path, is_stealth: bool, already: bool) -> Result<()> {
    // `state_repo::ensure` only materializes the symlink in non-stealth
    // master_url mode, so that is exactly the condition that owns no
    // project `.gitkeep` — stated here from config, order-independent.
    let config_path = root.join(".balls/config.json");
    let federated = !is_stealth && Config::load(&config_path)?.master_url().is_some();
    crate::gitignore::ensure_main_gitignore(root, is_stealth, federated)?;
    let keep_rel = Path::new(".balls/plugins/.gitkeep");
    let mut paths: Vec<&Path> = vec![Path::new(".balls/config.json"), Path::new(".gitignore")];
    if federated {
        git::git_rm_cached(root, &[keep_rel])?;
    } else {
        let abs_keep = root.join(keep_rel);
        if !abs_keep.exists() {
            fs::write(&abs_keep, "")?;
        }
        paths.push(keep_rel);
    }
    git::git_add(root, &paths)?;
    let msg = if already { "balls: reinitialize" } else { "balls: initialize" };
    git::git_commit(root, msg)?;
    Ok(())
}

#[cfg(test)]
#[path = "store_init_tests.rs"]
mod tests;
