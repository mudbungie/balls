use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Why discovery concluded the project isn't bl-initialized. Each
/// `NotInitialized` site constructs the variant matching its failure so the
/// message tells the user *which* of the look-alike failures they hit —
/// wrong directory, untracked repo, or a broken store — instead of one
/// catch-all "Run `bl init`" that answers none of those.
#[derive(Debug)]
pub enum NotInitKind {
    /// Walked from the start dir up to `/` and never found
    /// `.balls/config.json`. Carries every directory visited so the user
    /// can see exactly where bl looked (resolves "wrong directory?" at a
    /// glance).
    NoBallsOnWalk(Vec<PathBuf>),
    /// Inside a git repo whose main root has no `.balls/` at all.
    GitRepoNoBalls(PathBuf),
    /// `.balls/` exists but its non-stealth state worktree task dir is gone.
    StateWorktreeMissing { root: PathBuf, tasks_dir: PathBuf },
    /// no-git discovery found `.balls/config.json` but the store is
    /// unusable: either non-stealth (needs git) or its stealth tasks dir
    /// is missing.
    NoGitStoreUnusable { root: PathBuf, tasks_dir: PathBuf, stealth: bool },
    /// Config file absent at the resolved path.
    ConfigMissing(PathBuf),
}

#[derive(Debug)]
pub enum BallError {
    Io(io::Error),
    Json(serde_json::Error),
    Git(String),
    TaskNotFound(String),
    InvalidTask(String),
    NotInitialized(NotInitKind),
    NotARepo,
    AlreadyClaimed(String),
    DepsUnmet(String),
    NotClaimable(String),
    Cycle(String),
    WorktreeExists(PathBuf),
    Conflict(String),
    Other(String),
}

impl fmt::Display for BallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BallError::Io(e) => write!(f, "io error: {e}"),
            BallError::Json(e) => write!(f, "json error: {e}"),
            BallError::Git(s) => write!(f, "git error: {s}"),
            BallError::TaskNotFound(id) => write!(f, "task not found: {id}"),
            BallError::InvalidTask(s) => write!(f, "invalid task: {s}"),
            BallError::NotInitialized(k) => write!(f, "{k}"),
            BallError::NotARepo => write!(f, "not a git repository"),
            BallError::AlreadyClaimed(id) => write!(f, "task {id} is already claimed"),
            BallError::DepsUnmet(id) => write!(f, "task {id} has unmet dependencies"),
            BallError::NotClaimable(id) => write!(f, "task {id} is not claimable"),
            BallError::Cycle(s) => write!(f, "dependency cycle: {s}"),
            BallError::WorktreeExists(p) => {
                write!(f, "worktree already exists: {} (try `bl drop`)", p.display())
            }
            BallError::Conflict(s) => write!(f, "conflict: {s}"),
            BallError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl fmt::Display for NotInitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotInitKind::NoBallsOnWalk(searched) => {
                let chain = searched
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" → ");
                write!(
                    f,
                    "not initialized: no .balls/ found.\n  searched: {chain}\n  \
                     You may be in the wrong directory, or this project isn't \
                     tracked by bl yet — run `bl init` to start tracking it here."
                )
            }
            NotInitKind::GitRepoNoBalls(root) => write!(
                f,
                "not initialized: inside a git repo at {} but it has no .balls/.\n  \
                 This repo isn't bl-initialized — run `bl init` here, or check \
                 whether you meant a different project.",
                root.display()
            ),
            NotInitKind::StateWorktreeMissing { root, tasks_dir } => write!(
                f,
                "not initialized: .balls/ exists at {} but its task state is \
                 missing ({}).\n  The state worktree is absent or broken — run \
                 `bl repair` to rebuild it.",
                root.display(),
                tasks_dir.display()
            ),
            NotInitKind::NoGitStoreUnusable { root, tasks_dir, stealth: true } => write!(
                f,
                "not initialized: found .balls/ at {} but its stealth tasks \
                 directory is missing ({}).\n  The .balls/local/tasks_dir \
                 override points somewhere that no longer exists.",
                root.display(),
                tasks_dir.display()
            ),
            NotInitKind::NoGitStoreUnusable { root, stealth: false, .. } => write!(
                f,
                "not initialized: found .balls/ at {} but this is a git-backed \
                 store and no git repository is available here.\n  Run bl from \
                 inside the repo's work tree, or `bl init --tasks-dir` for a \
                 no-git store.",
                root.display()
            ),
            NotInitKind::ConfigMissing(path) => write!(
                f,
                "not initialized: no config at {}. Run `bl init`.",
                path.display()
            ),
        }
    }
}

impl BallError {
    pub fn no_balls_on_walk(searched: Vec<PathBuf>) -> Self {
        BallError::NotInitialized(NotInitKind::NoBallsOnWalk(searched))
    }
    pub fn git_repo_no_balls(root: &Path) -> Self {
        BallError::NotInitialized(NotInitKind::GitRepoNoBalls(root.to_path_buf()))
    }
    pub fn state_worktree_missing(root: &Path, tasks_dir: &Path) -> Self {
        BallError::NotInitialized(NotInitKind::StateWorktreeMissing {
            root: root.to_path_buf(),
            tasks_dir: tasks_dir.to_path_buf(),
        })
    }
    pub fn no_git_store_unusable(root: &Path, tasks_dir: &Path, stealth: bool) -> Self {
        BallError::NotInitialized(NotInitKind::NoGitStoreUnusable {
            root: root.to_path_buf(),
            tasks_dir: tasks_dir.to_path_buf(),
            stealth,
        })
    }
    pub fn config_missing(path: &Path) -> Self {
        BallError::NotInitialized(NotInitKind::ConfigMissing(path.to_path_buf()))
    }
}

impl std::error::Error for BallError {}

impl From<io::Error> for BallError {
    fn from(e: io::Error) -> Self {
        BallError::Io(e)
    }
}

impl From<serde_json::Error> for BallError {
    fn from(e: serde_json::Error) -> Self {
        BallError::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, BallError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_all_variants() {
        let cases: Vec<BallError> = vec![
            BallError::Io(io::Error::other("boom")),
            BallError::Json(serde_json::from_str::<i32>("x").unwrap_err()),
            BallError::Git("fatal".into()),
            BallError::TaskNotFound("bl-x".into()),
            BallError::InvalidTask("bad".into()),
            BallError::config_missing(Path::new("/x/config.json")),
            BallError::NotARepo,
            BallError::AlreadyClaimed("bl-x".into()),
            BallError::DepsUnmet("bl-x".into()),
            BallError::NotClaimable("bl-x".into()),
            BallError::Cycle("loop".into()),
            BallError::WorktreeExists(PathBuf::from("/tmp/x")),
            BallError::Conflict("merge".into()),
            BallError::Other("misc".into()),
        ];
        for e in &cases {
            let s = format!("{e}");
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn not_init_kinds_are_distinct_and_actionable() {
        let p = Path::new("/proj");
        let td = Path::new("/proj/.balls/worktree/.balls/tasks");
        let walk = vec![PathBuf::from("/a/b"), PathBuf::from("/a"), PathBuf::from("/")];
        let msgs: Vec<String> = [
            BallError::no_balls_on_walk(walk),
            BallError::git_repo_no_balls(p),
            BallError::state_worktree_missing(p, td),
            BallError::no_git_store_unusable(p, td, true),
            BallError::no_git_store_unusable(p, td, false),
            BallError::config_missing(Path::new("/proj/.balls/config.json")),
        ]
        .iter()
        .map(ToString::to_string)
        .collect();

        // Every site contains the legacy substring (back-compat with
        // callers/tests that scrape it) but a distinct, actionable tail.
        for m in &msgs {
            assert!(m.contains("not initialized"), "missing legacy prefix: {m}");
        }
        let unique: std::collections::HashSet<&String> = msgs.iter().collect();
        assert_eq!(unique.len(), msgs.len(), "messages must be distinct");

        // Spot-check the load-bearing details the ticket calls for.
        assert!(msgs[0].contains("searched: /a/b → /a → /"));
        assert!(msgs[1].contains("/proj") && msgs[1].contains("bl init"));
        assert!(msgs[2].contains("bl repair"));
        assert!(msgs[3].contains("stealth tasks directory"));
        assert!(msgs[4].contains("git-backed store"));
        assert!(msgs[5].contains("/proj/.balls/config.json"));
    }

    #[test]
    fn from_io_error() {
        let e: BallError = io::Error::other("x").into();
        assert!(matches!(e, BallError::Io(_)));
    }

    #[test]
    fn from_json_error() {
        let e: BallError = serde_json::from_str::<i32>("oops").unwrap_err().into();
        assert!(matches!(e, BallError::Json(_)));
    }

    #[test]
    fn error_is_std_error() {
        let e = BallError::NotARepo;
        let s: &dyn std::error::Error = &e;
        assert!(!s.to_string().is_empty());
    }
}
