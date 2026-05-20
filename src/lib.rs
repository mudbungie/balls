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

pub mod archive_recovery;
pub mod archived_child;
pub mod bare_squash;
pub mod claim_push;
pub mod claim_sync;
pub mod commit_msg;
pub mod commit_policy;
pub mod config;
pub mod delivery;
pub mod display;
pub mod doctor;
pub mod error;
pub mod git;
pub mod git_merge;
pub mod git_state;
mod hash;
pub mod human_gate;
pub mod link;
pub mod min_version;
pub mod negotiation;
pub mod participant;
pub mod participant_config;
pub mod plugin;
pub mod policy;
pub mod progress;
pub mod ready;
pub mod remaster;
pub mod render_list;
pub mod render_ready;
pub mod render_show;
pub mod render_show_relations;
#[cfg(test)]
mod render_show_test_support;
pub mod render_show_text;
pub mod repo_url;
pub mod resolve;
pub mod review;
pub mod review_deferred;
pub mod review_safety;
pub mod sanitize;
pub mod state_repo;
pub mod status;
pub mod store;
mod store_init;
mod store_lock;
mod store_paths;
pub mod sync_resolve;
pub mod task;
pub mod task_id;
pub mod task_io;
pub mod task_type;
pub mod task_validate;
pub mod tree;
#[cfg(test)]
mod tree_test_support;
pub mod worktree;
pub mod worktree_teardown;

pub use config::Config;
pub use error::{BallError, Result};
pub use store::Store;
pub use task::{
    validate_id, ArchivedChild, Link, LinkType, NewTaskOpts, Note, Status, Task, TaskType,
};
