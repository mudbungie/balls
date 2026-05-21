use super::*;
use tempfile::TempDir;

fn setup() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".balls")).unwrap();
    dir
}

#[test]
fn empty_pointer_when_no_files_present() {
    let dir = setup();
    let p = MasterPointer::load(dir.path()).unwrap();
    assert!(p.is_empty());
    assert_eq!(p.master_url(), None);
    assert_eq!(p.state_remote(), "origin");
}

#[test]
fn round_trips_via_save_and_load() {
    let dir = setup();
    let want = MasterPointer {
        master_url: Some("git@hub:org/repo.git".into()),
        state_remote: Some("upstream".into()),
    };
    want.save(dir.path()).unwrap();
    let got = MasterPointer::load(dir.path()).unwrap();
    assert_eq!(got, want);
}

#[test]
fn save_empty_removes_the_file() {
    let dir = setup();
    let p = MasterPointer::path(dir.path());
    fs::write(&p, r#"{"master_url":"x"}"#).unwrap();
    assert!(p.exists());
    MasterPointer::default().save(dir.path()).unwrap();
    assert!(!p.exists(), "empty save must clear the pointer file");
}

#[test]
fn legacy_fallback_extracts_master_url_and_state_remote_from_config() {
    let dir = setup();
    fs::write(
        dir.path().join(".balls/config.json"),
        r#"{
            "version":1, "id_length":4, "stale_threshold_seconds":60,
            "worktree_dir":".balls-worktrees",
            "master_url":"https://example.com/repo.git",
            "state_remote":"upstream"
        }"#,
    )
    .unwrap();
    let p = MasterPointer::load(dir.path()).unwrap();
    assert_eq!(p.master_url(), Some("https://example.com/repo.git"));
    assert_eq!(p.state_remote.as_deref(), Some("upstream"));
}

#[test]
fn pointer_file_wins_over_legacy_canonical() {
    let dir = setup();
    fs::write(
        dir.path().join(".balls/config.json"),
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,
            "worktree_dir":".balls-worktrees","master_url":"legacy"}"#,
    )
    .unwrap();
    MasterPointer {
        master_url: Some("new".into()),
        state_remote: None,
    }
    .save(dir.path())
    .unwrap();
    let p = MasterPointer::load(dir.path()).unwrap();
    assert_eq!(p.master_url(), Some("new"));
}

#[test]
fn load_or_empty_swallows_broken_pointer() {
    let dir = setup();
    fs::write(MasterPointer::path(dir.path()), "not json").unwrap();
    let p = MasterPointer::load_or_empty(dir.path());
    assert!(p.is_empty());
}

#[test]
fn legacy_fallback_silent_on_unreadable_canonical() {
    let dir = setup();
    fs::write(dir.path().join(".balls/config.json"), "garbage").unwrap();
    let p = MasterPointer::load(dir.path()).unwrap();
    assert!(p.is_empty());
}

#[test]
fn state_remote_defaults_to_origin() {
    let p = MasterPointer::default();
    assert_eq!(p.state_remote(), "origin");
}
