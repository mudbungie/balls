//! CLI command implementations, grouped by lifecycle phase.

use ball::error::Result;
use ball::store::Store;
use std::env;

pub mod basic;
pub mod lifecycle;
pub mod sync;

pub use basic::{cmd_create, cmd_init, cmd_list, cmd_ready, cmd_show};
pub use lifecycle::{cmd_claim, cmd_close, cmd_dep, cmd_drop, cmd_link, cmd_review, cmd_update};
pub use sync::{cmd_prime, cmd_repair, cmd_resolve, cmd_sync};

pub(crate) fn discover() -> Result<Store> {
    let cwd = env::current_dir()?;
    Store::discover(&cwd)
}

pub(crate) fn default_identity() -> String {
    env::var("BALL_IDENTITY")
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| "unknown".to_string())
}
