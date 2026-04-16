use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "bl", version, about = "Git-native task tracker", long_about = None)]
pub struct Cli {
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
        /// Task type: epic, task, bug
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
    },

    /// Show tasks ready to be claimed.
    Ready {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        no_fetch: bool,
    },

    /// Claim a task: update the task file and create a worktree.
    Claim {
        id: String,
        #[arg(long = "as")]
        identity: Option<String>,
    },

    /// Submit work for review: merge to main, keep worktree for rework.
    Review {
        id: String,
        #[arg(short = 'm', long)]
        message: Option<String>,
    },

    /// Close a reviewed task: archive and remove worktree. Must run from repo root.
    Close {
        id: String,
        #[arg(short = 'm', long)]
        message: Option<String>,
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
    /// Print dependency tree. Without ID, prints full graph.
    Tree { id: Option<String> },
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
