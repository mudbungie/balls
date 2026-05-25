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
    /// Opt in to cross-repo `delivered_in` resolution on local miss
    /// (bl-e454). Mirrors `bl show --resolve-remote`: when the target
    /// branch on this clone doesn't carry the `[bl-xxxx]` squash, fetch
    /// `delivered_repo` into the balls-owned code-refs cache and re-run
    /// the tag scan. Off by default — silent in deferred mode, where
    /// resolution auto-engages because the clone closing the task is
    /// typically not the one that produced the squash.
    #[arg(long = "resolve-remote")]
    pub resolve_remote: bool,
    /// Force a remote round-trip on this close. Mirrors `bl claim --sync`.
    #[arg(long, conflicts_with = "no_sync")]
    pub sync: bool,
    /// Skip any configured remote round-trip on this close.
    #[arg(long, conflicts_with = "sync")]
    pub no_sync: bool,
    #[command(flatten)]
    pub participant: ParticipantFlags,
}

/// `bl repair` arguments. Flattened into `Command::Repair` so the
/// enum stays under the 300-line cap (mirrors `CloseArgs`).
#[derive(Args, Debug)]
pub struct RepairFlags {
    #[arg(long)]
    pub fix: bool,
    /// Retract a stale half-push warning for ID. Writes a
    /// `state: forget-half-push <id>` commit on the state branch so
    /// the detector stops flagging it. ID must currently be flagged.
    /// Repeatable.
    #[arg(long = "forget-half-push", value_name = "ID")]
    pub forget_half_push: Vec<String>,
    /// Retract every half-push warning currently detected.
    #[arg(long = "forget-all-half-pushes", conflicts_with = "forget_half_push")]
    pub forget_all_half_pushes: bool,
    /// Move per-clone state under `~/.local/state/balls/` to this
    /// clone's current `<nested-clone-path>`. Use after a clone `mv`
    /// to re-bind the orphaned subtrees `bl doctor` reports. Refuses
    /// if the destination already exists with content.
    /// (SPEC-clone-layout §8 / §14.14, Phase 3 / bl-05e5.)
    #[arg(long = "rebind-path")]
    pub rebind_path: bool,
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
pub enum PluginCmd {
    /// Enable a plugin: insert/replace the effective entry and create
    /// the per-plugin config file if it does not exist.
    Enable {
        /// Plugin name. Becomes the key in the `plugins` map.
        name: String,
        /// Relative path under the plugins root for the per-plugin
        /// JSON config. Defaults to `<name>.json`.
        #[arg(long = "config-file", value_name = "PATH")]
        config_file: Option<String>,
        /// (Deprecated) Subscribe this plugin to the SPEC §11 legacy
        /// create/update path. Use `bl plugin policy` to set explicit
        /// per-event policy instead. Off by default.
        #[arg(long = "sync-on-change")]
        sync_on_change: bool,
    },
    /// Remove a plugin from the effective `plugins` map. The
    /// per-plugin config file is kept so credentials survive a
    /// temporary disable.
    Disable { name: String },
    /// Show the effective plugins map, sourced from `.balls/project.json`.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Set, drop, or clear a plugin's SPEC §11 per-event participant
    /// policy. Exactly one form per call.
    Policy {
        /// Plugin name. Must already be in the effective plugins map.
        name: String,
        /// `EVENT=KIND` — upsert one per-event subscription.
        /// Repeatable. EVENT is one of claim/review/close/update/
        /// sync/create/drop; KIND is required/best-effort/gating.
        #[arg(value_name = "EVENT=KIND", group = "policy_op")]
        set: Vec<String>,
        /// Drop one event's subscription. Repeatable. The participant
        /// block stays present — use --clear to remove it entirely.
        #[arg(long = "rm", value_name = "EVENT", group = "policy_op")]
        rm: Vec<String>,
        /// Remove the whole participant block: fall back to the
        /// legacy sync_on_change mapping.
        #[arg(long, group = "policy_op")]
        clear: bool,
        /// Write an explicit empty subscriptions map: suppress the
        /// legacy fallback so the plugin participates in nothing.
        #[arg(long = "no-legacy", group = "policy_op")]
        no_legacy: bool,
    },
    /// Show one plugin's effective entry and resolved per-event
    /// participant policy.
    Show {
        /// Plugin name.
        name: String,
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
