//! `bl ready` implementation. Split from `basic.rs` to keep both
//! files under the 300-line cap.

use super::{default_identity, discover};
use balls::display;
use balls::error::{BallError, Result};
use balls::git;
use balls::ready;
use balls::render_ready;
use balls::store::Store;
use balls::task::Task;
use std::fs;

pub fn cmd_ready(json: bool, no_fetch: bool, limit: Option<usize>) -> Result<()> {
    if let Some(0) = limit {
        return Err(BallError::Other("--limit must be >= 1".into()));
    }
    let store = discover()?;
    let cfg = store.load_config()?;
    if cfg.auto_fetch_on_ready && !no_fetch {
        maybe_auto_fetch(&store, cfg.stale_threshold_seconds);
    }
    let tasks = store.all_tasks()?;
    let ready = ready::ready_queue(&tasks);
    let total = ready.len();
    let capped: Vec<&Task> = match limit {
        Some(n) => ready.into_iter().take(n).collect(),
        None => ready,
    };
    let hidden = total - capped.len();
    if json {
        println!("{}", serde_json::to_string_pretty(&capped)?);
    } else if capped.is_empty() {
        println!("No tasks ready.");
    } else {
        let me = default_identity();
        print!(
            "{}",
            render_ready::render(&capped, &tasks, display::global(), &me),
        );
        if hidden > 0 {
            println!("... and {hidden} more. Raise --limit to see more.");
        }
    }
    Ok(())
}

fn maybe_auto_fetch(store: &Store, stale_threshold_seconds: u64) {
    if store.no_git {
        return;
    }
    let last_fetch = store.local_dir().join("last_fetch");
    let stale = match fs::metadata(&last_fetch).and_then(|m| m.modified()) {
        Ok(t) => std::time::SystemTime::now()
            .duration_since(t)
            .map(|d| d.as_secs() > stale_threshold_seconds)
            .unwrap_or(true),
        Err(_) => true,
    };
    if stale && git::git_has_remote(&store.root, "origin") {
        let _ = git::git_fetch(&store.root, "origin");
        let _ = fs::write(&last_fetch, "");
    }
}
