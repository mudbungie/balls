//! Ball: git-native task tracker for parallel agent workflows.
//!
//! Tasks are JSON files committed to your repo. Worktrees provide isolation.
//! Git provides sync, history, and collaboration. There is no database, no
//! daemon, no external service.
//!
//! # Library usage
//!
//! ```no_run
//! use balls::{Store, Task};
//! use std::env;
//!
//! let store = Store::discover(&env::current_dir().unwrap()).unwrap();
//! let tasks = store.all_tasks().unwrap();
//! for t in balls::ready::ready_queue(&tasks) {
//!     println!("[P{}] {} {}", t.priority, t.id, t.title);
//! }
//! ```

pub mod archived_child;
pub mod claim_sync;
pub mod commit_msg;
pub mod commit_policy;
pub mod config;
pub mod delivery;
pub mod display;
pub mod error;
pub mod git;
pub mod git_state;
pub mod link;
pub mod negotiation;
pub mod participant;
pub mod participant_config;
pub mod plugin;
pub mod policy;
pub mod progress;
pub mod ready;
pub mod render_list;
pub mod render_ready;
pub mod render_show;
pub mod render_show_text;
pub mod resolve;
pub mod review;
pub mod store;
mod store_init;
mod store_paths;
pub mod sync_resolve;
pub mod task;
pub mod task_io;
pub mod task_type;
pub mod tree;
pub mod worktree;

pub use config::Config;
pub use error::{BallError, Result};
pub use store::Store;
pub use task::{
    validate_id, ArchivedChild, Link, LinkType, NewTaskOpts, Note, Status, Task, TaskType,
};
