use super::*;
use std::fs;
use tempfile::TempDir;

fn tmp() -> TempDir {
    tempfile::Builder::new().prefix("balls-bc-").tempdir().unwrap()
}

#[test]
fn write_then_read_roundtrips() {
    let dir = tmp();
    let claims = dir.path().join("claims");
    let clone = std::path::PathBuf::from("/home/u/dev/proj");
    write_at(&claims, &clone).unwrap();
    let bc = read_at(&claims).expect("breadcrumb must read back");
    assert_eq!(bc.path, "/home/u/dev/proj");
    assert!(!bc.hostname.is_empty());
}

#[test]
fn read_missing_returns_none() {
    let dir = tmp();
    assert!(read_at(dir.path()).is_none());
}

#[test]
fn read_corrupt_returns_none() {
    let dir = tmp();
    fs::create_dir_all(dir.path()).unwrap();
    fs::write(breadcrumb_path(dir.path()), "{not json").unwrap();
    assert!(read_at(dir.path()).is_none());
}

#[test]
fn write_creates_missing_claims_dir() {
    let dir = tmp();
    let claims = dir.path().join("nested/claims");
    assert!(!claims.exists());
    write_at(&claims, std::path::Path::new("/x")).unwrap();
    assert!(claims.exists());
    assert!(breadcrumb_path(&claims).exists());
}

#[test]
fn backfill_writes_when_missing() {
    let dir = tmp();
    let claims = dir.path().join("claims");
    fs::create_dir_all(&claims).unwrap();
    backfill(&claims, std::path::Path::new("/clone/path")).unwrap();
    let bc = read_at(&claims).unwrap();
    assert_eq!(bc.path, "/clone/path");
}

#[test]
fn backfill_skips_when_present() {
    let dir = tmp();
    let claims = dir.path().join("claims");
    fs::create_dir_all(&claims).unwrap();
    write_at(&claims, std::path::Path::new("/original")).unwrap();
    backfill(&claims, std::path::Path::new("/different")).unwrap();
    let bc = read_at(&claims).unwrap();
    assert_eq!(bc.path, "/original", "back-fill must not overwrite an existing breadcrumb");
}

#[test]
fn backfill_skips_when_claims_dir_missing() {
    let dir = tmp();
    let claims = dir.path().join("absent");
    backfill(&claims, std::path::Path::new("/x")).unwrap();
    assert!(!claims.exists(), "back-fill must not materialize an absent claims dir");
}

#[test]
fn hostname_returns_a_value() {
    // Whichever branch fires — gethostname success, env fallback,
    // or unknown — the returned string must be non-empty so the
    // breadcrumb has a stable hostname field.
    assert!(!hostname().is_empty());
}

#[test]
fn parse_gethostname_rejects_failure_rc() {
    assert!(super::parse_gethostname(-1, b"laptop\0").is_none());
}

#[test]
fn parse_gethostname_rejects_unterminated_buffer() {
    // A buffer with no null byte cannot be parsed as a C string;
    // the fallback fires.
    assert!(super::parse_gethostname(0, &[b'a'; 32]).is_none());
}

#[test]
fn parse_gethostname_rejects_empty_string() {
    assert!(super::parse_gethostname(0, b"\0extra").is_none());
}

#[test]
fn parse_gethostname_returns_string_on_success() {
    let got = super::parse_gethostname(0, b"laptop\0extra").unwrap();
    assert_eq!(got, "laptop");
}

#[test]
fn env_hostname_from_uses_env_value_when_set() {
    assert_eq!(super::env_hostname_from(Some("custom".into())), "custom");
}

#[test]
fn env_hostname_from_falls_back_to_unknown() {
    assert_eq!(super::env_hostname_from(None), "unknown");
}

#[test]
fn env_hostname_reads_process_env_without_panic() {
    // Coverage gate: the one-liner that reads the process env. We
    // don't assert a specific value — the env may or may not have
    // HOSTNAME — but the call must succeed.
    let _ = super::env_hostname();
}

#[test]
fn nested_for_strips_leading_slash() {
    let n = nested_for(std::path::Path::new("/a/b/c"));
    assert_eq!(n, std::path::PathBuf::from("a/b/c"));
}

#[test]
fn breadcrumb_path_appends_filename_to_dir() {
    let p = breadcrumb_path(std::path::Path::new("/x/y"));
    assert_eq!(p, std::path::PathBuf::from("/x/y/clone-path.json"));
}
