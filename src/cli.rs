use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "bl", version, about = "Git-native task tracker", long_about = None)]
pub struct Cli {
    /// Force plain output: no color, no Unicode glyphs. Overrides
    /// terminal detection.
    #[arg(long, global = true)]
    pub plain: bool,

    #[command(subcommand)]
    pub command: Command,
}

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
    },

    /// Create a new task.
    Create {
        /// Task title
        title: String,
        /// Priority: 1 (highest) to 4 (lowest)
        #[arg(short = 'p', long, default_value_t = 3)]
        priority: u8,
        /// Task type label. Free-form identifier; common values:
        /// task, bug, epic, feature, chore, spike, question,
        /// discussion, retro. Only `epic` has special rendering.
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
        /// Include closed tasks
        #[arg(long)]
        all: bool,
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
    },

    /// Show tasks ready to be claimed.
    Ready {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        no_fetch: bool,
        /// Cap the number of entries shown. Text mode appends a
        /// `... and N more` footer when the queue is longer; JSON
        /// returns an array of at most this length. Must be >= 1.
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
    },

    /// Submit work for review: merge to main, keep worktree for rework.
    Review {
        id: String,
        #[arg(short = 'm', long)]
        message: Option<String>,
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
    },

    /// Close a reviewed task: archive and remove worktree. Must run from repo root.
    Close {
        id: String,
        #[arg(short = 'm', long)]
        message: Option<String>,
        #[arg(long = "as")]
        identity: Option<String>,
        /// Force a remote round-trip on this close. Mirrors
        /// `bl claim --sync`.
        #[arg(long, conflicts_with = "no_sync")]
        sync: bool,
        /// Skip any configured remote round-trip on this close.
        #[arg(long, conflicts_with = "sync")]
        no_sync: bool,
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
