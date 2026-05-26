//! Shared command-layer plumbing. The lifecycle handlers in this
//! module's siblings (`claim`, `lifecycle`, `basic`, `update`,
//! `sync`) lean on a few identical glue sequences — sync-flag
//! decoding, the state-branch event dance, the plugin-sync fan-out.
//! Factoring them here keeps each handler to its own logic and gives
//! every shared literal exactly one home.

use balls::config::Config;
use balls::error::Result;
use balls::participant::Event;
use balls::participant_config::{override_tokens, InvocationOverrides};
use balls::plugin::{self, Rollback, SyncReport};
use balls::policy::{LocalConfig, SyncOverride};
use balls::store::Store;
use balls::task::Task;

/// Load the config inputs every sync-policy decision needs: the CLI
/// `--sync`/`--no-sync` override, the repo-default `Config`, and the
/// per-clone `LocalConfig`. `claim`, `review` and `close` each layer
/// their own `require_remote_on_*` field and `policy::resolve*` call
/// on top of this shared front half.
pub(crate) fn sync_inputs(
    store: &Store,
    sync: bool,
    no_sync: bool,
) -> Result<(SyncOverride, Option<Config>, Option<LocalConfig>)> {
    let cli = SyncOverride::from_flags(sync, no_sync);
    let cfg = store.load_config().ok();
    let local = LocalConfig::load(store)?;
    Ok((cli, cfg, local))
}

/// Run one state-branch lifecycle event end to end: snapshot the
/// rollback point, let `mutate` perform the event commit and yield
/// the `(pre-image, post-image)` task pair, then dispatch the SPEC
/// §11 push with that snapshot as the rewind target. Confines the
/// `state_head` → `finish` → `Rollback::State` dance — and the wide
/// `plugin::finish` signature — to this one seam. `bl claim` is the
/// deliberate exception: it rewinds via `Rollback::DropClaim`, so it
/// calls `plugin::finish` directly.
pub(crate) fn finish_state_event(
    store: &Store,
    event: Event,
    identity: &str,
    overrides: &InvocationOverrides,
    sync: bool,
    no_sync: bool,
    mutate: impl FnOnce() -> Result<(Option<Task>, Task)>,
) -> Result<()> {
    // Snapshot the state-branch tip before `mutate` writes the event
    // commit — the rewind target on a required veto (SPEC §9).
    let rb = plugin::state_head(store)?;
    let (before, task) = mutate()?;
    let tokens = override_tokens(overrides, sync, no_sync);
    plugin::finish(
        store,
        before.as_ref(),
        &task,
        event,
        identity,
        overrides,
        &tokens,
        Rollback::State(rb.as_deref()),
    )?;
    Ok(())
}

/// Fire the plugin sync fan-out and hand each plugin's `SyncReport`
/// to `per_report`. Owns the two literals every sync entry point
/// shares — the dispatch-failure warning and the closing
/// `sync complete` — so callers can't drift them. The per-report
/// action is the caller's.
pub(crate) fn dispatch_sync_each(
    store: &Store,
    filter: Option<&str>,
    ident: &str,
    per_report: &mut dyn FnMut(&str, &SyncReport),
) {
    match plugin::dispatch_sync(store, filter, ident) {
        Ok(reports) => {
            for (name, report) in reports {
                per_report(&name, &report);
            }
        }
        Err(e) => eprintln!("warning: plugin sync failed: {e}"),
    }
    eprintln!("sync complete");
}
