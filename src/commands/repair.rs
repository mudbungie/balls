//! `bl repair` — half-push retraction, task-file scan, orphan cleanup.

use super::discover;
use super::half_push::{detect_half_push, write_forget_half_push};
use balls::error::{BallError, Result};
use balls::task::Task;
use balls::worktree;
use std::fs;

pub fn cmd_repair(
    fix: bool,
    forget_half_push: Vec<String>,
    forget_all_half_pushes: bool,
) -> Result<()> {
    let store = discover()?;

    // Forget actions run first: they're surgical and only touch the
    // state branch. They don't depend on, or interact with, the
    // task-file scan or orphan-cleanup paths below.
    if !forget_half_push.is_empty() || forget_all_half_pushes {
        if store.no_git || store.stealth {
            return Err(BallError::Other(
                "--forget-half-push requires a non-stealth git-backed repo".into(),
            ));
        }
        let flagged = detect_half_push(&store)?;
        let targets: Vec<String> = if forget_all_half_pushes {
            flagged.clone()
        } else {
            for id in &forget_half_push {
                if !flagged.contains(id) {
                    return Err(BallError::Other(format!(
                        "{id} is not a currently-flagged half-push; nothing to forget"
                    )));
                }
            }
            forget_half_push.clone()
        };
        if targets.is_empty() {
            println!("No half-push warnings to forget.");
        } else {
            write_forget_half_push(&store, &targets)?;
            for id in &targets {
                println!("forgot half-push: {id}");
            }
        }
        return Ok(());
    }

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
    if fix && !store.no_git {
        let (rc, rw) = worktree::cleanup_orphans(&store)?;
        for id in &rc {
            println!("removed orphan claim: {id}");
        }
        for id in &rw {
            println!("removed orphan worktree: {id}");
        }
    }
    Ok(())
}
