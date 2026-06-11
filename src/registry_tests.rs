//! Tests for the §6 local binary binding. Each builds a throwaway landing
//! checkout in a tempdir and drives the binding store against it with real
//! symlinks — the layer's whole job is `bin/<name>` filesystem manipulation.

use super::*;
use std::fs;
use tempfile::TempDir;

/// A registry over a fresh temp landing dir, plus its root for asserting on the
/// on-disk layout directly.
fn fixture() -> (TempDir, Registry) {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    (tmp, reg)
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
fn resolve_bin_reads_a_bound_name_through_to_its_binary() {
    let (tmp, reg) = fixture();
    let bin = tmp.path().join("real-binary");
    fs::write(&bin, "x").unwrap();
    reg.bind("tracker", &bin).unwrap();
    assert_eq!(reg.resolve_bin("tracker"), Some(fs::canonicalize(&bin).unwrap()));
}

#[test]
fn resolve_bin_is_none_for_an_unbound_or_dangling_name() {
    let (tmp, reg) = fixture();
    // Never bound here.
    assert_eq!(reg.resolve_bin("ghost"), None);
    // Bound, but the target was removed → dangling → None (canonicalize fails).
    let bin = tmp.path().join("gone");
    fs::write(&bin, "x").unwrap();
    reg.bind("tracker", &bin).unwrap();
    fs::remove_file(&bin).unwrap();
    assert_eq!(reg.resolve_bin("tracker"), None);
}

#[test]
fn resolve_bin_is_none_when_the_name_resolves_to_a_non_file() {
    // bl-bee0: an empty name joins to bin/ itself, which canonicalizes fine
    // (it is a directory) and then execs with EACCES. Only a regular file is
    // a binding — anything else is the clean §6 dangling-entry `None`.
    let (tmp, reg) = fixture();
    fs::create_dir_all(tmp.path().join("config/plugins/bin")).unwrap();
    assert_eq!(reg.resolve_bin(""), None);
    // A binding hand-pointed at a directory is equally not a binary.
    reg.bind("dir", tmp.path()).unwrap();
    assert_eq!(reg.resolve_bin("dir"), None);
}

#[test]
fn binding_is_idempotent_and_re_bind_wins() {
    let (tmp, reg) = fixture();
    let first = tmp.path().join("one");
    let second = tmp.path().join("two");
    fs::write(&first, "1").unwrap();
    fs::write(&second, "2").unwrap();

    reg.bind("tracker", &first).unwrap();
    reg.bind("tracker", &second).unwrap(); // re-bind replaces

    let link = tmp.path().join("config/plugins/bin/tracker");
    assert_eq!(fs::read_link(&link).unwrap(), second);
}
