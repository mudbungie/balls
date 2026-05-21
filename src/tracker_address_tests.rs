use super::*;
use crate::git::clean_git_command;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    assert!(clean_git_command(dir).args(args).output().unwrap().status.success());
}

fn repo() -> TempDir {
    let d = TempDir::new().unwrap();
    git(d.path(), &["init", "-q"]);
    d
}

#[test]
fn implicit_default_no_origin_is_offline_bootstrappable() {
    let d = repo();
    let a = resolve(d.path(), &Config::default());
    assert_eq!(a, Address { url: None, branch: "balls/tasks".into(), explicit: false });
}

#[test]
fn implicit_default_resolves_origin_live() {
    let d = repo();
    git(d.path(), &["remote", "add", "origin", "https://code/repo.git"]);
    let a = resolve(d.path(), &Config::default());
    assert_eq!(a.url.as_deref(), Some("https://code/repo.git"));
    assert!(!a.explicit);
}

#[test]
fn explicit_state_url_and_branch_win() {
    let d = repo();
    let cfg = Config {
        state_url: Some("ssh://hub/t.git".into()),
        state_branch: Some("custom/tasks".into()),
        ..Config::default()
    };
    let a = resolve(d.path(), &cfg);
    assert_eq!(
        a,
        Address { url: Some("ssh://hub/t.git".into()), branch: "custom/tasks".into(), explicit: true }
    );
}

#[test]
fn ensure_supported_accepts_absent_state_branch() {
    ensure_supported(&Config::default()).unwrap();
}

#[test]
fn ensure_supported_accepts_explicit_default_branch() {
    let cfg = Config { state_branch: Some(DEFAULT_BRANCH.into()), ..Config::default() };
    ensure_supported(&cfg).unwrap();
}

#[test]
fn ensure_supported_rejects_non_default_branch() {
    let cfg = Config { state_branch: Some("project-x".into()), ..Config::default() };
    let err = ensure_supported(&cfg).unwrap_err();
    assert!(
        matches!(&err, BallError::Other(s)
            if s.contains("state_branch") && s.contains("not yet supported")),
        "expected an unsupported-state_branch error, got: {err:?}"
    );
}

#[test]
fn legacy_master_json_pointer_is_read_transparently() {
    let d = repo();
    fs::create_dir_all(d.path().join(".balls")).unwrap();
    fs::write(d.path().join(".balls/master.json"), r#"{"master_url":"git://hub/x"}"#).unwrap();
    let a = resolve(d.path(), &Config::default());
    assert_eq!(a.url.as_deref(), Some("git://hub/x"));
    assert!(a.explicit);
}

#[test]
fn legacy_in_config_master_url_is_explicit() {
    let d = repo();
    let cfg = Config { master_url: Some("git://legacy/hub".into()), ..Config::default() };
    let a = resolve(d.path(), &cfg);
    assert_eq!(a.url.as_deref(), Some("git://legacy/hub"));
    assert!(a.explicit);
}

#[test]
fn legacy_state_remote_name_resolves_to_url_safe_but_unlinked() {
    let d = repo();
    git(d.path(), &["remote", "add", "hub", "https://hub/s.git"]);
    let cfg = Config { state_remote: Some("hub".into()), ..Config::default() };
    let a = resolve(d.path(), &cfg);
    assert_eq!(a.url.as_deref(), Some("https://hub/s.git"));
    assert!(!a.explicit, "a legacy state_remote stays safe-but-unlinked");
}

#[test]
fn legacy_state_remote_unresolvable_falls_to_implicit_default() {
    let d = repo();
    let cfg = Config { state_remote: Some("nonexistent".into()), ..Config::default() };
    let a = resolve(d.path(), &cfg);
    assert_eq!(a.url, None);
    assert!(!a.explicit);
}

#[test]
fn corrupt_master_json_degrades_to_standalone() {
    let d = repo();
    fs::create_dir_all(d.path().join(".balls")).unwrap();
    fs::write(d.path().join(".balls/master.json"), "not json").unwrap();
    let a = resolve(d.path(), &Config::default());
    assert_eq!(a.url, None);
}
