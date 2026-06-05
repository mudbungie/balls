//! Tests for the §2/§6 symlink registry. Each builds a throwaway operating
//! checkout in a tempdir and drives the registry against it with real
//! symlinks — the layer's whole job is filesystem manipulation.

use super::*;
use std::fs;
use tempfile::TempDir;

/// A registry over a fresh temp operating dir, plus the dir's root path for
/// asserting on the on-disk layout directly.
fn fixture() -> (TempDir, Registry) {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    (tmp, reg)
}

fn phase_dir(root: &Path, op: &str, phase: &str) -> PathBuf {
    root.join("config").join("plugins").join(op).join(phase)
}

#[test]
fn link_writes_a_relative_symlink_in_nn_name_form() {
    let (tmp, reg) = fixture();
    reg.link("close", "pre", 0, "tracker").unwrap();

    let entry = phase_dir(tmp.path(), "close", "pre").join("00-tracker");
    assert_eq!(fs::read_link(&entry).unwrap(), Path::new("../../bin/tracker"));
}

#[test]
fn bind_writes_an_absolute_symlink_to_the_machine_binary() {
    let (tmp, reg) = fixture();
    let bin = tmp.path().join("real-binary");
    fs::write(&bin, "#!/bin/sh\n").unwrap();
    reg.bind("tracker", &bin).unwrap();

    let link = tmp.path().join("config/plugins/bin/tracker");
    assert_eq!(fs::read_link(&link).unwrap(), bin);
}

#[test]
fn the_two_level_stitch_resolves_through_to_the_binary() {
    // The committed relative link plus the local absolute link compose: the
    // `../../bin/<name>` depth is exactly what makes the chain resolve.
    let (tmp, reg) = fixture();
    let bin = tmp.path().join("real-binary");
    fs::write(&bin, "#!/bin/sh\n").unwrap();
    reg.link("close", "pre", 0, "tracker").unwrap();
    reg.bind("tracker", &bin).unwrap();

    let entry = phase_dir(tmp.path(), "close", "pre").join("00-tracker");
    assert_eq!(fs::canonicalize(&entry).unwrap(), fs::canonicalize(&bin).unwrap());
}

#[test]
fn resolve_returns_entries_in_run_order_with_resolved_bins() {
    let (tmp, reg) = fixture();
    let bin = tmp.path().join("real-binary");
    fs::write(&bin, "x").unwrap();
    // Wire out of order; bind only one of the two.
    reg.link("close", "pre", 2, "second").unwrap();
    reg.link("close", "pre", 0, "first").unwrap();
    reg.bind("first", &bin).unwrap();

    let refs = reg.resolve("close", "pre").unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].order, 0);
    assert_eq!(refs[0].name, "first");
    assert_eq!(refs[0].bin, Some(fs::canonicalize(&bin).unwrap()));
    // `second` is wired but not installed here → dangling.
    assert_eq!(refs[1].name, "second");
    assert_eq!(refs[1].bin, None);
}

#[test]
fn equal_order_breaks_ties_by_name() {
    let (_tmp, reg) = fixture();
    reg.link("close", "pre", 0, "beta").unwrap();
    reg.link("close", "pre", 0, "alpha").unwrap();

    let names: Vec<_> = reg
        .resolve("close", "pre")
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
}

#[test]
fn an_absent_phase_dir_resolves_to_nothing() {
    let (_tmp, reg) = fixture();
    assert!(reg.resolve("close", "pre").unwrap().is_empty());
}

#[test]
fn an_empty_phase_dir_resolves_to_nothing() {
    let (tmp, reg) = fixture();
    fs::create_dir_all(phase_dir(tmp.path(), "close", "pre")).unwrap();
    assert!(reg.resolve("close", "pre").unwrap().is_empty());
}

#[test]
fn non_entry_filenames_are_ignored() {
    let (tmp, reg) = fixture();
    let dir = phase_dir(tmp.path(), "close", "pre");
    fs::create_dir_all(&dir).unwrap();
    // No dash; empty name; non-numeric prefix — none are registry entries.
    for bad in ["noseparator", "00-", "xx-tracker"] {
        fs::write(dir.join(bad), "").unwrap();
    }
    reg.link("close", "pre", 1, "good").unwrap();

    let refs = reg.resolve("close", "pre").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].name, "good");
}

#[test]
fn linking_and_binding_are_idempotent() {
    let (tmp, reg) = fixture();
    let first = tmp.path().join("one");
    let second = tmp.path().join("two");
    fs::write(&first, "1").unwrap();
    fs::write(&second, "2").unwrap();

    reg.link("close", "pre", 0, "tracker").unwrap();
    reg.link("close", "pre", 0, "tracker").unwrap(); // replaces, no error
    reg.bind("tracker", &first).unwrap();
    reg.bind("tracker", &second).unwrap(); // re-bind wins

    let link = tmp.path().join("config/plugins/bin/tracker");
    assert_eq!(fs::read_link(&link).unwrap(), second);
}
