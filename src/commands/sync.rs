//! sync, resolve, prime, repair — remote reconciliation and agent bootstrap.

use super::{default_identity, discover};
use super::sync_report::apply_sync_report;
use balls::error::{BallError, Result};
use balls::git;
use balls::plugin;
use balls::ready;
use balls::resolve;
use balls::store::Store;
use balls::task::{Status, Task};
use balls::worktree;
use std::fs;
use std::path::{Path, PathBuf};

pub fn cmd_sync(remote: String, task_filter: Option<String>) -> Result<()> {
    let store = discover()?;
    let has_remote = git::git_has_remote(&store.root, &remote);
    if has_remote {
        sync_with_remote(&store, &remote)?;
    }
    match plugin::run_plugin_sync(&store, task_filter.as_deref()) {
        Ok(reports) => {
            for (plugin_name, report) in reports {
                if let Err(e) = apply_sync_report(&store, &plugin_name, &report) {
                    eprintln!("warning: failed to apply {} sync report: {}", plugin_name, e);
                }
            }
        }
        Err(e) => {
            eprintln!("warning: plugin sync failed: {}", e);
        }
    }
    println!("sync complete");
    Ok(())
}

fn sync_with_remote(store: &Store, remote: &str) -> Result<()> {
    if !git::git_fetch(&store.root, remote)? {
        eprintln!("warning: fetch failed, continuing offline");
    }

    let branch = git::git_current_branch(&store.root)?;
    let remote_branch = format!("{}/{}", remote, branch);

    fetch_merge_resolve(store, remote, &remote_branch)?;

    if git::git_push(&store.root, remote, &branch).is_err() {
        fetch_merge_resolve(store, remote, &remote_branch)?;
        git::git_push(&store.root, remote, &branch)?;
    }
    Ok(())
}

/// Fetch from `remote`, merge `remote_branch` into the current branch, and
/// auto-resolve any task-file conflicts. Tolerates a missing upstream branch.
fn fetch_merge_resolve(store: &Store, remote: &str, remote_branch: &str) -> Result<()> {
    let _ = git::git_fetch(&store.root, remote);
    match git::git_merge(&store.root, remote_branch, None) {
        Ok(git::MergeResult::Conflict) => {
            auto_resolve_conflicts(store)?;
            git::git_commit(&store.root, "balls: auto-resolve sync conflicts")?;
        }
        Ok(_) => {}
        Err(_) => {
            // Remote branch may not exist yet; that's fine.
        }
    }
    Ok(())
}

fn auto_resolve_conflicts(store: &Store) -> Result<()> {
    let conflicted = git::git_list_conflicted_files(&store.root)?;
    for path in conflicted {
        let rel = path.strip_prefix(&store.root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy();
        if !rel_str.starts_with(".balls/tasks/") || !rel_str.ends_with(".json") {
            return Err(BallError::Conflict(format!(
                "unhandled conflict in {}",
                path.display()
            )));
        }
        let content = fs::read_to_string(&path)?;
        let (ours, theirs) = resolve::parse_conflict_markers(&content)?;
        let merged = resolve::resolve_conflict(&ours, &theirs);
        merged.save(&path)?;
        let rel_p = Path::new(&*rel_str).to_path_buf();
        git::git_add(&store.root, &[rel_p.as_path()])?;
    }
    Ok(())
}

pub fn cmd_resolve(file: String) -> Result<()> {
    let path = PathBuf::from(&file);
    let content = fs::read_to_string(&path)?;
    let (ours, theirs) = resolve::parse_conflict_markers(&content)?;
    let merged = resolve::resolve_conflict(&ours, &theirs);
    merged.save(&path)?;
    println!("resolved {}", file);
    Ok(())
}

pub fn cmd_prime(identity: Option<String>, json: bool) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);

    // Try to sync; ignore failure
    let _ = cmd_sync("origin".to_string(), None);

    let tasks = store.all_tasks()?;
    let ready_tasks = ready::ready_queue(&tasks);
    let claimed: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.claimed_by.as_deref() == Some(&ident))
        .filter(|t| t.status == Status::InProgress)
        .collect();

    if json {
        let obj = serde_json::json!({
            "identity": ident,
            "claimed": claimed,
            "ready": ready_tasks,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
        return Ok(());
    }

    println!("=== balls prime: {} ===", ident);
    for t in &claimed {
        let wt_dir = store
            .worktrees_root()
            .map(|r| r.join(&t.id))
            .unwrap_or_default();
        println!(
            "Claimed (resume): {} \"{}\" @ {}",
            t.id,
            t.title,
            wt_dir.display()
        );
    }
    println!("Ready:");
    for t in &ready_tasks {
        println!("  [P{}] {} \"{}\"", t.priority, t.id, t.title);
    }
    println!("===");
    Ok(())
}

pub fn cmd_repair(fix: bool) -> Result<()> {
    let store = discover()?;
    let dir = store.tasks_dir();
    let mut bad = Vec::new();
    if dir.exists() {
        for e in fs::read_dir(&dir)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Err(err) = Task::load(&p) {
                bad.push((p, err.to_string()));
            }
        }
    }
    if bad.is_empty() {
        println!("All task files OK.");
    } else {
        for (p, e) in &bad {
            println!("BAD: {} - {}", p.display(), e);
        }
    }
    if fix {
        let (rc, rw) = worktree::cleanup_orphans(&store)?;
        for id in &rc {
            println!("removed orphan claim: {}", id);
        }
        for id in &rw {
            println!("removed orphan worktree: {}", id);
        }
    }
    Ok(())
}
