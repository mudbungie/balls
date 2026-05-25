//! CLI command implementations, grouped by lifecycle phase.

use balls::error::Result;
use balls::store::Store;
use std::env;

pub mod basic;
pub mod claim;
pub mod completions;
pub mod dep_link;
pub mod doctor;
pub mod init;
mod half_push;
pub mod lifecycle;
pub mod migrate;
pub mod plugin;
mod plumbing;
mod prime_status;
pub mod ready_cmd;
pub mod remaster;
pub mod repair;
pub mod sync;
mod sync_bounds;
mod sync_report;
mod sync_review;
mod sync_targets;
pub mod update;

pub use basic::{cmd_create, cmd_list, cmd_show, CreateArgs};
pub use init::cmd_init;
pub use migrate::cmd_migrate;
pub use completions::{install_completions, uninstall_completions};
pub use dep_link::{cmd_dep, cmd_link};
pub use doctor::cmd_doctor;
pub use claim::cmd_claim;
pub use lifecycle::{cmd_close, cmd_drop, cmd_review};
pub use plugin::{
    cmd_plugin_disable, cmd_plugin_enable, cmd_plugin_list, cmd_plugin_policy, cmd_plugin_show,
};
pub use ready_cmd::cmd_ready;
pub use remaster::cmd_remaster;
pub use repair::{cmd_repair, RepairArgs};
pub use sync::{cmd_prime, cmd_resolve, cmd_sync, SyncArgs};
pub use update::cmd_update;

pub(crate) fn discover() -> Result<Store> {
    let cwd = env::current_dir()?;
    Store::discover(&cwd)
}

pub(crate) fn default_identity() -> String {
    env::var("BALLS_IDENTITY")
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| "unknown".to_string())
}
