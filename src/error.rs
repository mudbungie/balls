use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum BallError {
    Io(io::Error),
    Json(serde_json::Error),
    Git(String),
    TaskNotFound(String),
    InvalidTask(String),
    NotInitialized,
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
            BallError::NotInitialized => {
                write!(f, "not initialized. Run `bl init`")
            }
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
            BallError::NotInitialized,
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
