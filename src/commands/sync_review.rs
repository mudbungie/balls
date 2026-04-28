//! `bl sync --review` and friends: staged sync-report subcommands. The
//! dispatcher in `commands::sync` routes here when one of the
//! human-gate flags is set; everything in this module assumes the
//! reader already chose the staged path. Live sync stays in
//! `commands::sync`.

use super::sync_report::apply_sync_report;
use super::{default_identity, discover};
use balls::error::Result;
use balls::store::Store;
use balls::{human_gate, plugin};
use std::fs;

/// Run the standalone sync event with staging instead of apply: each
/// plugin's `SyncReport` lands on disk under
/// `.balls/local/pending-sync/sync/<id>.json`. State-branch sync is
/// skipped — staging is a pre-apply hold, not a remote-reconciliation
/// step. Operators replay or discard via the other entry points.
pub fn stage_sync_event(remote: &str, task_filter: Option<&str>) -> Result<()> {
    let _ = remote; // remote round-trip is intentionally suppressed in --review
    let store = discover()?;
    let ident = default_identity();
    dispatch_and_stage(&store, task_filter, &ident);
    eprintln!("sync complete");
    Ok(())
}

fn dispatch_and_stage(store: &Store, task_filter: Option<&str>, ident: &str) {
    match plugin::dispatch_sync(store, task_filter, ident) {
        Ok(reports) => {
            for (plugin_name, report) in reports {
                match human_gate::stage_sync(store, &plugin_name, &report) {
                    Ok(id) => eprintln!("staged plugin {plugin_name} sync report at {id}"),
                    Err(e) => eprintln!("warning: stage {plugin_name} failed: {e}"),
                }
            }
        }
        Err(e) => eprintln!("warning: plugin sync failed: {e}"),
    }
}

/// Apply a previously staged report by id. Replays it through the
/// normal `apply_sync_report` path so per-item warn-and-continue
/// semantics match live sync, then removes the staged file. Removal
/// happens unconditionally on a successful load: the per-item warnings
/// `apply_sync_report` emits aren't apply *failures*, just diagnostics.
pub fn apply_staged(id: &str) -> Result<()> {
    let store = discover()?;
    let entry = human_gate::load_staged(&store, id)?;
    apply_sync_report(&store, &entry.data.plugin, &entry.data.report);
    fs::remove_file(&entry.path)?;
    eprintln!(
        "applied staged {} report from plugin {}",
        entry.data.event, entry.data.plugin
    );
    Ok(())
}

/// Drop a staged report without replaying it. Errs if the id has
/// already been applied or discarded so an operator typo doesn't
/// silently succeed.
pub fn discard_staged(id: &str) -> Result<()> {
    let store = discover()?;
    let (event, plugin) = human_gate::discard_staged(&store, id)?;
    eprintln!("discarded staged {event} report from plugin {plugin}");
    Ok(())
}

/// Print a one-line summary per staged entry, or a placeholder when
/// nothing is pending. Output goes to stdout so the listing is
/// pipeable; status messages from the other commands stay on stderr.
pub fn list_staged() -> Result<()> {
    let store = discover()?;
    let entries = human_gate::list_staged(&store)?;
    if entries.is_empty() {
        println!("(no staged sync reports)");
        return Ok(());
    }
    for e in &entries {
        let r = &e.data.report;
        println!(
            "{} {} {} created={} updated={} deleted={}",
            e.id,
            e.data.event,
            e.data.plugin,
            r.created.len(),
            r.updated.len(),
            r.deleted.len(),
        );
    }
    Ok(())
}
