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

pub mod config;
pub mod error;
pub mod git;
pub mod plugin;
pub mod ready;
pub mod resolve;
pub mod store;
pub mod task;
pub mod worktree;

pub use config::Config;
pub use error::{BallError, Result};
pub use store::Store;
pub use task::{ArchivedChild, Link, LinkType, NewTaskOpts, Note, Status, Task, TaskType};
