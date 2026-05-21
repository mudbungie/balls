//! Subcommand and value enums split out of `cli.rs` to keep that file
//! under the 300-line cap. Re-exported from `cli` so `main.rs`'s
//! `use cli::{...}` is unaffected.

use balls::participant_config::InvocationOverrides;
use clap::{Args, Subcommand, ValueEnum};

/// SPEC §11 per-invocation participant overrides, flattened into every
/// lifecycle command. `--sync`/`--no-sync` stay per-command (they
/// predate this and are git-remote-specific); these two cover the
/// plugin participants. Repeatable. Applied tokens are logged in the
/// state-branch commit message via `participant_config::override_log`.
#[derive(Args, Debug, Default, Clone)]
pub struct ParticipantFlags {
    /// Drop participant NAME from this event's negotiation (SPEC §11).
    #[arg(long = "skip", value_name = "NAME")]
    pub skip: Vec<String>,
    /// Force participant NAME to required for this event (SPEC §11).
    #[arg(long = "required", value_name = "NAME")]
    pub required: Vec<String>,
}

impl ParticipantFlags {
    pub fn overrides(&self) -> InvocationOverrides {
        InvocationOverrides {
            skip: self.skip.iter().cloned().collect(),
            required: self.required.iter().cloned().collect(),
        }
    }
}

/// `bl close` arguments, split here so `cli.rs`'s `Command` enum stays
/// under the 300-line cap (mirrors `CreateArgs`/`SyncArgs`). Flattened
/// into `Command::Close`.
#[derive(Args, Debug)]
pub struct CloseArgs {
    /// Reviewer message, embedded in the state-branch close commit
    /// body. Repeatable, like `git commit -m … -m …`: each value
    /// becomes its own paragraph.
    #[arg(short = 'm', long = "message")]
    pub message: Vec<String>,
    #[arg(long = "as")]
    pub identity: Option<String>,
    /// Override the delivering commit instead of tag-scanning the
    /// target branch (SPEC §6; bl-87ea). Use when the forge produced a
    /// rebase-merge with several commits and you want a specific one.
    #[arg(long = "delivered", value_name = "SHA")]
    pub delivered: Option<String>,
    /// Override the `delivered_repo` provenance instead of auto-tagging
    /// the current clone's `origin` URL (bl-733e). Use when closing on
    /// behalf of another repo — e.g. a bridge clone running close from
    /// a forge-sync hook for a sha that lives in a different client.
    /// Pairs with `--delivered`; takes effect alone when correcting
    /// the source repo of an already-set `delivered_in`.
    #[arg(long = "delivered-repo", value_name = "URL")]
    pub delivered_repo: Option<String>,
    /// Force a remote round-trip on this close. Mirrors `bl claim --sync`.
    #[arg(long, conflicts_with = "no_sync")]
    pub sync: bool,
    /// Skip any configured remote round-trip on this close.
    #[arg(long, conflicts_with = "sync")]
    pub no_sync: bool,
    #[command(flatten)]
    pub participant: ParticipantFlags,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ShellArg {
    Bash,
    Zsh,
    Fish,
}

#[derive(Subcommand, Debug)]
pub enum DepCmd {
    /// Add a dependency: TASK depends on DEPENDS_ON.
    Add { task: String, depends_on: String },
    /// Remove a dependency.
    Rm { task: String, depends_on: String },
    /// Print parent/child tree with box-drawing. Deps and gates show
    /// as inline annotations, never as nesting. Without ID, prints
    /// every parentless task as its own root.
    Tree {
        id: Option<String>,
        /// Emit a nested JSON tree instead of the box-drawn text.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum LinkCmd {
    /// Add a typed link: relates_to, duplicates, supersedes, replies_to, gates.
    /// `gates` blocks the source task from closing until the target closes.
    Add {
        task: String,
        link_type: String,
        target: String,
    },
    /// Remove a typed link.
    Rm {
        task: String,
        link_type: String,
        target: String,
    },
}
