//! The `bl` subcommand enum, split out of `cli.rs` to keep that file
//! under the 300-line cap. Re-exported from `cli` so `main.rs`'s
//! `use cli::{...}` is unaffected.

use clap::Subcommand;

use crate::cli_sub::{CloseArgs, DepCmd, LinkCmd, ParticipantFlags, PluginCmd, ShellArg};

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize balls in the current git repository.
    Init {
        /// Stealth mode: store tasks outside the repo (not git-tracked).
        #[arg(long)]
        stealth: bool,
        /// Custom absolute path for task storage. Implies --stealth.
        #[arg(long)]
        tasks_dir: Option<String>,
        /// Bootstrap a bare clone: bare-clone SOURCE into
        /// CLONE_DIR/.git and reconstruct the loose store there.
        /// Idempotent. SOURCE's `main` must already be
        /// balls-initialized (run `bl init` in a working clone and
        /// push first). Mutually exclusive with --stealth/--tasks-dir.
        #[arg(long, num_args = 2, value_names = ["SOURCE", "CLONE_DIR"])]
        bare: Option<Vec<String>>,
    },

    /// Create a new task.
    Create {
        /// Task title
        title: String,
        /// Priority: 1 (highest) to 4 (lowest)
        #[arg(short = 'p', long, default_value_t = 3)]
        priority: u8,
        /// Task type label (free-form: task, bug, epic, feature,
        /// chore, …). Only `epic` has special rendering.
        #[arg(short = 't', long, default_value = "task")]
        task_type: String,
        /// Parent task ID
        #[arg(long)]
        parent: Option<String>,
        /// Dependency task ID (repeatable)
        #[arg(long = "dep")]
        dep: Vec<String>,
        /// Tag (repeatable)
        #[arg(long = "tag")]
        tag: Vec<String>,
        /// Description
        #[arg(short = 'd', long, default_value = "")]
        description: String,
        /// Integration branch this task's `bl review` squashes into,
        /// overriding the repo `target_branch` and current-branch
        /// fallback (e.g. a hotfix targeting `main` in a develop repo).
        #[arg(long = "target-branch")]
        target_branch: Option<String>,
        #[command(flatten)]
        participant: ParticipantFlags,
    },

    /// List tasks.
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Filter by priority
        #[arg(short = 'p', long)]
        priority: Option<u8>,
        /// Filter by parent
        #[arg(long)]
        parent: Option<String>,
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
        /// Open and closed tasks together (closed ones reconstructed
        /// from `balls/tasks` history — high-volume on old repos).
        #[arg(long)]
        all: bool,
        /// Only closed/archived tasks, reconstructed from the
        /// `balls/tasks` history.
        #[arg(long)]
        closed: bool,
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// Show details of a task. For closed/review tasks, prints a `delivered:` line resolving the squash-merge commit on main.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
        /// Append absolute ISO timestamps next to the relative ones.
        #[arg(long)]
        verbose: bool,
        /// Opt in to cross-repo delivery resolution (bl-f37b): on a
        /// local miss, fetch `delivered_repo` into a balls-owned
        /// cache and re-run the tag scan. Off by default — fetches
        /// from arbitrary forge URLs are rude without the operator
        /// asking for them.
        #[arg(long)]
        resolve_remote: bool,
    },

    /// Show tasks ready to be claimed.
    Ready {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        no_fetch: bool,
        /// Cap entries shown (text mode appends a `... and N more`
        /// footer; JSON returns at most this many). Must be >= 1.
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Claim a task: update the task file and create a worktree.
    Claim {
        id: String,
        #[arg(long = "as")]
        identity: Option<String>,
        /// Claim without creating a git worktree (status flip only).
        #[arg(long)]
        no_worktree: bool,
        /// Force a remote round-trip on this claim. Overrides repo
        /// and per-clone config. Closes the offline-agent claim race
        /// at the cost of requiring network.
        #[arg(long, conflicts_with = "no_sync")]
        sync: bool,
        /// Skip any configured remote round-trip on this claim. Lets
        /// you claim offline against a repo whose maintainer set
        /// `require_remote_on_claim`.
        #[arg(long, conflicts_with = "sync")]
        no_sync: bool,
        #[command(flatten)]
        participant: ParticipantFlags,
    },

    /// Submit work for review: merge to main, keep worktree for rework.
    Review {
        id: String,
        /// Commit message. Repeatable, like `git commit -m … -m …`:
        /// the first `-m` is the title (under ~50 chars), each later
        /// `-m` becomes a body paragraph with a blank line between.
        /// A single value may also span multiple lines. The `[bl-id]`
        /// delivery tag is appended to the title automatically.
        #[arg(short = 'm', long = "message")]
        message: Vec<String>,
        #[arg(long = "as")]
        identity: Option<String>,
        /// Force a remote round-trip on this review. Mirrors
        /// `bl claim --sync`; flips the per-event sync policy on for
        /// just this invocation.
        #[arg(long, conflicts_with = "no_sync")]
        sync: bool,
        /// Skip any configured remote round-trip on this review.
        #[arg(long, conflicts_with = "sync")]
        no_sync: bool,
        #[command(flatten)]
        participant: ParticipantFlags,
    },

    /// Close a reviewed task: archive and remove worktree. Must run from repo root.
    Close {
        id: String,
        #[command(flatten)]
        args: CloseArgs,
    },

    /// Drop a claim: reset task and remove worktree.
    Drop {
        id: String,
        #[arg(long)]
        force: bool,
    },

    /// Update fields of a task.
    Update {
        id: String,
        /// field=value pairs
        assignments: Vec<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "as")]
        identity: Option<String>,
        #[command(flatten)]
        participant: ParticipantFlags,
    },

    /// Manage dependencies.
    Dep {
        #[command(subcommand)]
        sub: DepCmd,
    },

    /// Manage typed links (relates_to, duplicates, supersedes, replies_to, gates).
    Link {
        #[command(subcommand)]
        sub: LinkCmd,
    },

    /// Manage the effective plugins map (enable/disable/list/policy/show).
    Plugin {
        #[command(subcommand)]
        sub: PluginCmd,
    },

    /// Sync with remote: fetch, merge, resolve, push.
    Sync {
        #[arg(long, default_value = "origin")]
        remote: String,
        /// Sync a single task by local ID or remote key.
        #[arg(long)]
        task: Option<String>,
        /// Stage every plugin sync report for human review under
        /// `.balls/local/pending-sync/sync/` instead of applying it.
        /// Use `--apply <id>` or `--discard <id>` to act on a staged
        /// entry afterward.
        #[arg(long, conflicts_with_all = ["apply", "discard", "list_staged"])]
        review: bool,
        /// Apply a previously staged sync report by id and remove the
        /// staged file.
        #[arg(long, value_name = "ID", conflicts_with_all = ["discard", "list_staged"])]
        apply: Option<String>,
        /// Drop a staged sync report without applying it.
        #[arg(long, value_name = "ID", conflicts_with = "list_staged")]
        discard: Option<String>,
        /// List staged sync reports awaiting review.
        #[arg(long)]
        list_staged: bool,
    },

    /// Merge a task file with git conflict markers using balls' field-level rules. Rarely needed — `bl sync` runs this automatically.
    Resolve { file: String },

    /// Prime an agent session: sync and print ready + in-progress tasks.
    Prime {
        #[arg(long = "as")]
        identity: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Read-only health check: report repo/bl state drift and the
    /// command that fixes each. Changes nothing.
    Doctor,

    /// Scan and repair malformed task files and orphaned state.
    Repair {
        #[arg(long)]
        fix: bool,
        /// Retract a stale half-push warning for ID. Writes a
        /// `state: forget-half-push <id>` commit on the state branch
        /// so the detector stops flagging it. ID must currently be
        /// flagged. Repeatable.
        #[arg(long = "forget-half-push", value_name = "ID")]
        forget_half_push: Vec<String>,
        /// Retract every half-push warning currently detected.
        #[arg(
            long = "forget-all-half-pushes",
            conflicts_with = "forget_half_push"
        )]
        forget_all_half_pushes: bool,
    },

    /// Re-point this repo's task state branch at TARGET (a tracker
    /// URL) and reconcile local-only tasks onto it, writing the
    /// address to `.balls/config.json`. `--commit` also commits.
    /// `--detach` clears the address and returns to standalone.
    Remaster {
        /// Tracker URL whose state branch becomes authoritative.
        /// Omit only with `--detach`.
        target: Option<String>,
        /// State branch on the tracker (SPEC §5/§8). Default
        /// `balls/tasks`. Persists in `config.json`; `--detach` clears.
        #[arg(long = "branch", value_name = "B", conflicts_with = "detach")]
        branch: Option<String>,
        /// Also `git commit` the `config.json` address change.
        #[arg(long, conflicts_with = "detach")]
        commit: bool,
        /// Sever shared history and the link: go standalone again.
        #[arg(long)]
        detach: bool,
    },

    /// Print the agent skill guide (SKILL.md).
    Skill,

    /// Generate shell completions, or install/uninstall to ~/.local/share.
    Completions {
        /// Shell to generate completions for. Omit when using `--install`/`--uninstall`.
        shell: Option<ShellArg>,
        /// Install bash, zsh, and fish completions to standard XDG paths.
        #[arg(long, conflicts_with_all = ["shell", "uninstall"])]
        install: bool,
        /// Remove completions previously written by `--install`.
        #[arg(long, conflicts_with_all = ["shell", "install"])]
        uninstall: bool,
    },
}
