//! §6 install tests. Each builds throwaway `from`/`to` checkout roots in a
//! tempdir and drives the path-copy + the local resolve/bind directly — the
//! layer's whole job is shape-decided filesystem manipulation plus a `protocol`
//! round-trip.

#![cfg(unix)]

use super::copy::matches;
use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;

/// A `from` root, a `to` root, and helpers to populate / inspect committed files.
struct Roots {
    _tmp: TempDir,
    from: PathBuf,
    to: PathBuf,
}

impl Roots {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join("from");
        let to = tmp.path().join("to");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        Self { _tmp: tmp, from, to }
    }

    /// Write a regular file at `root/<rel>` with `body`, making parents.
    fn file(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    /// Read `to/<rel>` as a string (panics if absent).
    fn read_to(&self, rel: &str) -> String {
        fs::read_to_string(self.to.join(rel)).unwrap()
    }

    fn has_to(&self, rel: &str) -> bool {
        self.to.join(rel).exists()
    }
}

const SCHEDULE: &str = "[hooks]\n\"close.pre\" = [\"tracker\"]\n\"claim.post\" = [\"tracker\"]\n";

#[test]
fn the_matcher_handles_literal_prefix_suffix_and_multiple_stars() {
    assert!(matches("*", "anything"));
    assert!(matches("bl-1234.md", "bl-1234.md"));
    assert!(!matches("bl-1234.md", "bl-9999.md"));
    assert!(matches("bl-*.md", "bl-abcd.md"));
    assert!(!matches("bl-*.md", "note.txt"));
    assert!(matches("*-*.md", "bl-x.md"));
    assert!(!matches("bl-*", "note.txt"));
}

#[test]
fn the_default_path_is_config_so_the_bare_bundle_excludes_the_sibling_store() {
    assert_eq!(DEFAULT_PATH, "config");
}

#[test]
fn a_folder_mirror_makes_the_destination_identical_deleting_what_the_source_lacks() {
    let r = Roots::new();
    Roots::file(&r.from, "config/balls.toml", "publish = true\n");
    Roots::file(&r.from, "config/plugins.toml", SCHEDULE);
    Roots::file(&r.to, "config/balls.toml", "publish = false\n");
    Roots::file(&r.to, "config/stale.toml", "remove me\n"); // source lacks this

    let summary = install("config", &r.from, &r.to).unwrap();

    assert_eq!(r.read_to("config/balls.toml"), "publish = true\n"); // overwritten
    assert_eq!(r.read_to("config/plugins.toml"), SCHEDULE); // added
    assert!(!r.has_to("config/stale.toml")); // deletion propagated
    assert_eq!(summary, Summary { added: 2, deleted: 1 });
}

#[test]
fn a_folder_mirror_leaves_a_sibling_path_untouched() {
    let r = Roots::new();
    Roots::file(&r.from, "config/balls.toml", "x = 1\n");
    Roots::file(&r.to, "tasks/bl-keep.md", "the co-resident store"); // sibling of config/

    install("config", &r.from, &r.to).unwrap();

    assert!(r.has_to("config/balls.toml"));
    assert_eq!(r.read_to("tasks/bl-keep.md"), "the co-resident store"); // never eaten
}

#[test]
fn a_folder_mirror_never_copies_or_deletes_the_gitignored_bin() {
    let r = Roots::new();
    Roots::file(&r.from, "config/plugins.toml", SCHEDULE);
    Roots::file(&r.from, "config/plugins/bin/tracker", "SOURCE bin — must not travel");
    Roots::file(&r.to, "config/plugins/bin/local", "this box's symlink — must survive");

    let summary = install("config", &r.from, &r.to).unwrap();

    assert!(!r.has_to("config/plugins/bin/tracker")); // never copied
    assert_eq!(r.read_to("config/plugins/bin/local"), "this box's symlink — must survive");
    assert_eq!(summary, Summary { added: 1, deleted: 0 }); // bin counted on neither side
}

#[test]
fn a_folder_mirror_from_an_absent_source_empties_the_destination_scope() {
    let r = Roots::new();
    Roots::file(&r.to, "tasks/bl-aaaa.md", "a");
    Roots::file(&r.to, "tasks/bl-bbbb.md", "b");

    let summary = install("tasks", &r.from, &r.to).unwrap(); // source has no tasks/

    assert!(!r.has_to("tasks/bl-aaaa.md"));
    assert!(!r.has_to("tasks/bl-bbbb.md"));
    assert_eq!(summary, Summary { added: 0, deleted: 2 });
}

#[test]
fn a_single_file_path_unions_overwriting_only_that_file() {
    let r = Roots::new();
    Roots::file(&r.from, "tasks/bl-1234.md", "ported");
    Roots::file(&r.to, "tasks/bl-1234.md", "old");
    Roots::file(&r.to, "tasks/bl-9999.md", "untouched");

    let summary = install("tasks/bl-1234.md", &r.from, &r.to).unwrap();

    assert_eq!(r.read_to("tasks/bl-1234.md"), "ported"); // source wins
    assert_eq!(r.read_to("tasks/bl-9999.md"), "untouched"); // sibling left alone
    assert_eq!(summary, Summary { added: 1, deleted: 0 });
}

#[test]
fn a_single_file_path_with_an_absent_source_copies_nothing() {
    let r = Roots::new();
    let summary = install("tasks/bl-ghost.md", &r.from, &r.to).unwrap();
    assert!(!r.has_to("tasks/bl-ghost.md"));
    assert_eq!(summary, Summary::default());
}

#[test]
fn a_glob_path_unions_each_matching_file_and_leaves_others_alone() {
    let r = Roots::new();
    Roots::file(&r.from, "tasks/bl-aaaa.md", "a");
    Roots::file(&r.from, "tasks/bl-bbbb.md", "b");
    Roots::file(&r.from, "tasks/README", "not matched by *.md");
    fs::create_dir_all(r.from.join("tasks/sub")).unwrap(); // a dir never matches
    Roots::file(&r.to, "tasks/bl-cccc.md", "c"); // pre-existing, must survive

    let summary = install("tasks/*.md", &r.from, &r.to).unwrap();

    assert!(r.has_to("tasks/bl-aaaa.md"));
    assert!(r.has_to("tasks/bl-bbbb.md"));
    assert_eq!(r.read_to("tasks/bl-cccc.md"), "c"); // union: untouched
    assert!(!r.has_to("tasks/README")); // *.md did not match
    assert_eq!(summary, Summary { added: 2, deleted: 0 });
}

#[test]
fn a_glob_over_an_absent_source_dir_copies_nothing() {
    let r = Roots::new();
    let summary = install("tasks/*", &r.from, &r.to).unwrap();
    assert!(!r.has_to("tasks"));
    assert_eq!(summary, Summary::default());
}

#[test]
fn a_bare_pattern_globs_the_root() {
    let r = Roots::new();
    Roots::file(&r.from, "bl-top.md", "top-level");
    let summary = install("*.md", &r.from, &r.to).unwrap();
    assert_eq!(r.read_to("bl-top.md"), "top-level");
    assert_eq!(summary, Summary { added: 1, deleted: 0 });
}

#[test]
fn referenced_reads_the_landing_schedule_and_an_absent_one_references_nothing() {
    let r = Roots::new();
    assert!(referenced(&r.to).unwrap().is_empty()); // no schedule yet

    Roots::file(&r.from, "config/plugins.toml", SCHEDULE);
    install("config", &r.from, &r.to).unwrap();

    let refs = referenced(&r.to).unwrap();
    assert_eq!(refs["tracker"], BTreeSet::from(["close".to_string(), "claim".to_string()]));
}

#[test]
fn summary_renders_added_and_deleted() {
    assert_eq!(Summary { added: 3, deleted: 1 }.to_string(), "3 added / 1 deleted");
}

#[test]
fn install_error_displays_and_wraps_io() {
    let unsupported = InstallError::Unsupported {
        name: "tracker".into(),
        reason: "boom".into(),
    };
    assert!(unsupported.to_string().contains("tracker"));
    let io = InstallError::from(io::Error::new(io::ErrorKind::NotFound, "nope"));
    assert!(io.to_string().contains("nope"));
}

/// Write an executable `protocol`-answering plugin and return its path.
fn plugin(dir: &Path, name: &str, proto: &str, ops: &str) -> PathBuf {
    let path = dir.join(name);
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":{proto},\"ops\":{ops}}}'; fi\n"
    );
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn ops(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn resolve_validates_then_binds_the_local_binary() {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    let bin = plugin(tmp.path(), "tracker", "[1]", "[\"close\",\"claim\"]");
    resolve_and_bind(&reg, "tracker", &bin, &ops(&["close"]), 1).unwrap();
    let link = tmp.path().join("config/plugins/bin/tracker");
    assert_eq!(fs::read_link(link).unwrap(), bin);
}

#[test]
fn resolve_refuses_a_binary_that_does_not_speak_the_protocol() {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    let bin = plugin(tmp.path(), "tracker", "[2]", "[\"close\"]");
    let err = resolve_and_bind(&reg, "tracker", &bin, &ops(&["close"]), 1).unwrap_err();
    assert!(err.to_string().contains("protocol 1"));
}

#[test]
fn resolve_refuses_a_binary_that_does_not_handle_a_wired_op() {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    let bin = plugin(tmp.path(), "tracker", "[1]", "[\"close\"]");
    let err = resolve_and_bind(&reg, "tracker", &bin, &ops(&["claim"]), 1).unwrap_err();
    assert!(err.to_string().contains("op 'claim'"));
}

#[test]
fn resolve_surfaces_a_self_describe_failure() {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    let missing = tmp.path().join("does-not-exist");
    assert!(resolve_and_bind(&reg, "ghost", &missing, &ops(&["close"]), 1).is_err());
}
