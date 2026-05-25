use super::*;
use crate::clone_breadcrumb;
use crate::xdg_paths::XdgBases;
use std::fs;
use tempfile::TempDir;

fn tmp() -> TempDir {
    tempfile::Builder::new().prefix("balls-moved-").tempdir().unwrap()
}

fn bases_at(home: &Path) -> XdgBases {
    XdgBases::with(home, None, None, None)
}

#[test]
fn no_claims_dir_yields_no_orphans() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/proj");
    fs::create_dir_all(&clone).unwrap();
    assert!(find_orphans(&bases, &clone).is_empty());
}

#[test]
fn no_breadcrumbs_yields_no_orphans() {
    let home = tmp();
    let bases = bases_at(home.path());
    let claims_root = bases.state_root().join("claims");
    fs::create_dir_all(claims_root.join("home/u/x")).unwrap();
    let clone = home.path().join("home/u/x");
    assert!(find_orphans(&bases, &clone).is_empty());
}

#[test]
fn matching_nested_is_not_an_orphan() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/here");
    fs::create_dir_all(&clone).unwrap();
    let claims = bases.state_root().join("claims").join(clone.strip_prefix("/").unwrap());
    fs::create_dir_all(&claims).unwrap();
    clone_breadcrumb::write_at(&claims, &clone).unwrap();
    assert!(find_orphans(&bases, &clone).is_empty());
}

#[test]
fn moved_clone_is_detected_with_orphan_task_ids() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/new");
    fs::create_dir_all(&clone).unwrap();
    // Old subtree under a different nested path, breadcrumb recording
    // the *current* clone path (the move scenario).
    let old_claims = bases.state_root().join("claims").join("home/u/old");
    fs::create_dir_all(&old_claims).unwrap();
    clone_breadcrumb::write_at(&old_claims, &clone).unwrap();
    fs::write(old_claims.join("bl-aaaa"), "x").unwrap();
    fs::write(old_claims.join("bl-bbbb"), "x").unwrap();

    let orphans = find_orphans(&bases, &clone);
    assert_eq!(orphans.len(), 1);
    let o = &orphans[0];
    assert_eq!(o.claims_dir, old_claims);
    assert_eq!(o.nested, std::path::PathBuf::from("home/u/old"));
    assert_eq!(o.orphan_task_ids, vec!["bl-aaaa".to_string(), "bl-bbbb".to_string()]);
    assert_eq!(o.recorded_path.as_deref(), Some(clone.to_string_lossy().as_ref()));
}

#[test]
fn cross_host_breadcrumb_is_filtered_out() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/here");
    fs::create_dir_all(&clone).unwrap();
    let other_claims = bases.state_root().join("claims").join("home/u/other-host");
    fs::create_dir_all(&other_claims).unwrap();
    let bc = clone_breadcrumb::CloneBreadcrumb {
        path: clone.to_string_lossy().into_owned(),
        hostname: "definitely-not-this-host-xyz".into(),
    };
    fs::write(
        clone_breadcrumb::breadcrumb_path(&other_claims),
        serde_json::to_string(&bc).unwrap(),
    )
    .unwrap();
    assert!(find_orphans(&bases, &clone).is_empty());
}

#[test]
fn breadcrumb_for_a_different_clone_is_not_an_orphan_of_this_one() {
    let home = tmp();
    let bases = bases_at(home.path());
    let me = home.path().join("dev/me");
    let other = home.path().join("dev/other");
    fs::create_dir_all(&me).unwrap();
    fs::create_dir_all(&other).unwrap();
    let other_claims = bases.state_root().join("claims").join("home/u/other");
    fs::create_dir_all(&other_claims).unwrap();
    // Breadcrumb names a different clone path → owned by that clone,
    // not orphaned from mine.
    clone_breadcrumb::write_at(&other_claims, &other).unwrap();
    assert!(find_orphans(&bases, &me).is_empty());
}

#[test]
fn to_findings_quotes_repair_command_and_orphan_ids() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/new");
    fs::create_dir_all(&clone).unwrap();
    let old_claims = bases.state_root().join("claims").join("home/u/old");
    fs::create_dir_all(&old_claims).unwrap();
    clone_breadcrumb::write_at(&old_claims, &clone).unwrap();
    fs::write(old_claims.join("bl-cafe"), "x").unwrap();

    let orphans = find_orphans(&bases, &clone);
    let findings = to_findings(&orphans, &clone);
    assert_eq!(findings.len(), 1);
    let f = &findings[0];
    assert!(f.problem.contains("moved clone detected"));
    assert!(f.problem.contains("bl-cafe"));
    let hint = f.hint.as_deref().unwrap();
    assert!(hint.contains("bl repair --rebind-path"));
    assert!(hint.contains(&clone.to_string_lossy().to_string()));
}

#[test]
fn to_findings_reports_no_claimed_tasks_when_empty() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/new");
    fs::create_dir_all(&clone).unwrap();
    let old_claims = bases.state_root().join("claims").join("home/u/empty");
    fs::create_dir_all(&old_claims).unwrap();
    clone_breadcrumb::write_at(&old_claims, &clone).unwrap();

    let orphans = find_orphans(&bases, &clone);
    let findings = to_findings(&orphans, &clone);
    assert!(findings[0].problem.contains("no claimed tasks"));
}

#[test]
fn unreadable_subdir_does_not_panic_the_walk() {
    // Coverage gate: the `Ok(entries) = fs::read_dir` else-arm is
    // taken when a subdir of `claims/` is replaced by a file. The
    // walk continues without panicking.
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/new");
    fs::create_dir_all(&clone).unwrap();
    let claims_root = bases.state_root().join("claims");
    fs::create_dir_all(&claims_root).unwrap();
    fs::write(claims_root.join("not-a-dir"), "x").unwrap();
    // Reaching this without panic is the assertion.
    let _ = find_orphans(&bases, &clone);
}
