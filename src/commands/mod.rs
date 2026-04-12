//! CLI command implementations, grouped by lifecycle phase.

use balls::error::Result;
use balls::store::Store;
use std::env;

pub mod basic;
pub mod completions;
pub mod lifecycle;
pub mod sync;
mod sync_report;

pub use basic::{cmd_create, cmd_init, cmd_list, cmd_ready, cmd_show};
pub use completions::{install_completions, uninstall_completions};
pub use lifecycle::{cmd_claim, cmd_close, cmd_dep, cmd_drop, cmd_link, cmd_review, cmd_update};
pub use sync::{cmd_prime, cmd_repair, cmd_resolve, cmd_sync};

pub(crate) fn discover() -> Result<Store> {
    let cwd = env::current_dir()?;
    Store::discover(&cwd)
}

pub(crate) fn default_identity() -> String {
    env::var("BALLS_IDENTITY")
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| "unknown".to_string())
}
