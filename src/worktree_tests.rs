use super::*;
use tempfile::tempdir;

#[test]
fn link_state_path_creates_symlink_in_empty_dir() {
    let td = tempdir().unwrap();
    let dst = td.path().join("local");
    link_state_path(PathBuf::from("/source/dir"), &dst).unwrap();
    assert!(dst.is_symlink());
    assert_eq!(fs::read_link(&dst).unwrap(), PathBuf::from("/source/dir"));
}

#[test]
fn link_state_path_idempotent_on_existing_symlink() {
    let td = tempdir().unwrap();
    let dst = td.path().join("local");
    std::os::unix::fs::symlink(PathBuf::from("/already/there"), &dst).unwrap();
    link_state_path(PathBuf::from("/something/else"), &dst).unwrap();
    assert_eq!(fs::read_link(&dst).unwrap(), PathBuf::from("/already/there"));
}

#[test]
fn link_state_path_rejects_pre_existing_regular_file() {
    let td = tempdir().unwrap();
    let dst = td.path().join("local");
    fs::write(&dst, "hostile").unwrap();
    let err = link_state_path(PathBuf::from("/x"), &dst).unwrap_err();
    assert!(matches!(err, BallError::Other(ref s) if s.contains("non-symlink")));
    assert!(!dst.is_symlink());
}

#[test]
fn link_state_path_rejects_pre_existing_directory() {
    let td = tempdir().unwrap();
    let dst = td.path().join("local");
    fs::create_dir(&dst).unwrap();
    let err = link_state_path(PathBuf::from("/x"), &dst).unwrap_err();
    assert!(matches!(err, BallError::Other(_)));
}

#[test]
fn link_state_path_idempotent_on_dangling_symlink() {
    // A dangling symlink is still a symlink; we accept it as
    // idempotent rather than overwriting, matching the strict
    // is_symlink-first semantics of ensure_tasks_symlink.
    let td = tempdir().unwrap();
    let dst = td.path().join("local");
    std::os::unix::fs::symlink(PathBuf::from("/does/not/exist"), &dst).unwrap();
    link_state_path(PathBuf::from("/replacement"), &dst).unwrap();
    assert_eq!(fs::read_link(&dst).unwrap(), PathBuf::from("/does/not/exist"));
}
