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

/// Write a breadcrumb at `claims_dir` recording an arbitrary path —
/// useful for simulating "moved away from <path>" without that path
/// having to exist on disk. `write_at` of [`crate::clone_breadcrumb`]
/// would only let you record the *current* clone; the moved-clone
/// scenarios need a recorded path that isn't the running clone, so
/// the tests write the JSON by hand.
fn seed_breadcrumb(claims_dir: &Path, recorded: &Path) {
    fs::create_dir_all(claims_dir).unwrap();
    let bc = clone_breadcrumb::CloneBreadcrumb {
        path: recorded.to_string_lossy().into_owned(),
        hostname: clone_breadcrumb::hostname(),
    };
    fs::write(
        clone_breadcrumb::breadcrumb_path(claims_dir),
        serde_json::to_string(&bc).unwrap(),
    )
    .unwrap();
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
    clone_breadcrumb::write_at(&claims, &clone).unwrap();
    assert!(find_orphans(&bases, &clone).is_empty());
}

#[test]
fn moved_clone_is_detected_with_orphan_task_ids() {
    let home = tmp();
    let bases = bases_at(home.path());
    // Current clone lives here on disk; the *old* path the user
    // moved away from never exists in this fixture (modelling the
    // gone-after-mv state).
    let clone = home.path().join("dev/new");
    fs::create_dir_all(&clone).unwrap();
    let old_recorded = home.path().join("dev/old");
    let old_claims = bases.state_root().join("claims").join("home/u/old");
    seed_breadcrumb(&old_claims, &old_recorded);
    fs::write(old_claims.join("bl-aaaa"), "x").unwrap();
    fs::write(old_claims.join("bl-bbbb"), "x").unwrap();

    let orphans = find_orphans(&bases, &clone);
    assert_eq!(orphans.len(), 1);
    let o = &orphans[0];
    assert_eq!(o.claims_dir, old_claims);
    assert_eq!(o.nested, std::path::PathBuf::from("home/u/old"));
    assert_eq!(o.orphan_task_ids, vec!["bl-aaaa".to_string(), "bl-bbbb".to_string()]);
    assert_eq!(o.recorded_path, old_recorded.to_string_lossy().to_string());
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
        path: home.path().join("dev/gone").to_string_lossy().into_owned(),
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
fn another_active_clone_on_disk_is_not_orphan_of_this_one() {
    // The disambiguation gate: a breadcrumb at a sibling nested path
    // recording a path that still exists on disk belongs to that live
    // clone, not orphaned by this clone's move. Without this check we
    // would surface every other active clone on the host as movable.
    let home = tmp();
    let bases = bases_at(home.path());
    let me = home.path().join("dev/me");
    let sibling = home.path().join("dev/sibling-still-here");
    fs::create_dir_all(&me).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    let sibling_claims = bases.state_root().join("claims").join("home/u/sibling");
    seed_breadcrumb(&sibling_claims, &sibling);
    assert!(find_orphans(&bases, &me).is_empty());
}

#[test]
fn breadcrumb_recording_current_clone_at_other_nested_is_not_orphan() {
    // Defensive: if a stray breadcrumb at some other nested path
    // happens to record *our* current clone path, that isn't a move —
    // it's a writer bug. Skip rather than surface a self-rebind.
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/here");
    fs::create_dir_all(&clone).unwrap();
    let stray_claims = bases.state_root().join("claims").join("home/u/stray");
    seed_breadcrumb(&stray_claims, &clone);
    assert!(find_orphans(&bases, &clone).is_empty());
}

#[test]
fn corrupt_breadcrumb_is_skipped() {
    // A breadcrumb that won't parse has no recorded path to classify
    // — find_orphans can't call it moved-from-here, so it doesn't.
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/here");
    fs::create_dir_all(&clone).unwrap();
    let bad_claims = bases.state_root().join("claims").join("home/u/bad");
    fs::create_dir_all(&bad_claims).unwrap();
    fs::write(clone_breadcrumb::breadcrumb_path(&bad_claims), "{not json").unwrap();
    assert!(find_orphans(&bases, &clone).is_empty());
}

#[test]
fn to_findings_quotes_repair_command_and_orphan_ids() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/new");
    fs::create_dir_all(&clone).unwrap();
    let old_recorded = home.path().join("dev/old");
    let old_claims = bases.state_root().join("claims").join("home/u/old");
    seed_breadcrumb(&old_claims, &old_recorded);
    fs::write(old_claims.join("bl-cafe"), "x").unwrap();

    let orphans = find_orphans(&bases, &clone);
    let findings = to_findings(&orphans, &clone);
    assert_eq!(findings.len(), 1);
    let f = &findings[0];
    assert!(f.problem.contains("moved clone detected"));
    assert!(f.problem.contains(&old_recorded.to_string_lossy().to_string()));
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
    let old_recorded = home.path().join("dev/empty-old");
    let old_claims = bases.state_root().join("claims").join("home/u/empty");
    seed_breadcrumb(&old_claims, &old_recorded);

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
