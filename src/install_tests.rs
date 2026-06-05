//! §6 install tests. Each builds throwaway `from`/`to` checkout roots in a
//! tempdir and drives the dir-based transfer + the local resolve/bind directly —
//! the layer's whole job is filesystem manipulation plus a `protocol` round-trip.

use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

/// A `from` root, a `to` root, and helpers to populate committed wiring + files.
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

    /// Place a relative symlink `config/plugins/<rel>` → `target` under `root`.
    fn wire(root: &Path, rel: &str, target: &str) {
        let link = root.join("config").join("plugins").join(rel);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(target, link).unwrap();
    }

    /// Write a regular file at `root/<rel>` with `body`, making parents.
    fn file(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
}

fn link_target(root: &Path, rel: &str) -> PathBuf {
    fs::read_link(root.join("config").join("plugins").join(rel)).unwrap()
}

#[test]
fn objects_round_trip_and_the_bundle_excludes_tasks() {
    for o in Object::ALL {
        assert_eq!(Object::parse(o.token()), Some(o));
    }
    assert_eq!(Object::parse("nope"), None);
    assert_eq!(Object::DEFAULT_BUNDLE, [Object::Plugins, Object::Config]);
}

#[test]
fn plugins_copies_wiring_and_records_each_op_it_is_wired_into() {
    let r = Roots::new();
    Roots::wire(&r.from, "close/pre/00-tracker", "../../bin/tracker");
    Roots::wire(&r.from, "claim/pre/00-tracker", "../../bin/tracker");
    let refs = transfer(&[Object::Plugins], &r.from, &r.to).unwrap();

    // The committed relative symlink travels verbatim.
    assert_eq!(link_target(&r.to, "close/pre/00-tracker"), Path::new("../../bin/tracker"));
    assert_eq!(link_target(&r.to, "claim/pre/00-tracker"), Path::new("../../bin/tracker"));
    // tracker is referenced, wired into both ops.
    assert_eq!(refs["tracker"], BTreeSet::from(["close".to_string(), "claim".to_string()]));
}

#[test]
fn the_wiring_only_invariant_excludes_bin_and_plugin_config_files() {
    let r = Roots::new();
    Roots::wire(&r.from, "close/pre/00-tracker", "../../bin/tracker");
    // The LOCAL absolute bin link and a plugin's committed config (the trail
    // pointer lives here) must NOT travel.
    let bin = r.from.join("config/plugins/bin");
    fs::create_dir_all(&bin).unwrap();
    symlink("/abs/path/to/tracker", bin.join("tracker")).unwrap();
    Roots::file(&r.from, "config/plugins/tracker/remote.toml", "next = \"git@x\"\n");

    transfer(&[Object::Plugins], &r.from, &r.to).unwrap();
    assert!(!r.to.join("config/plugins/bin").exists(), "bin/ excluded");
    assert!(!r.to.join("config/plugins/tracker/remote.toml").exists(), "pointer excluded");
}

#[test]
fn an_absolute_symlink_outside_bin_is_skipped() {
    let r = Roots::new();
    Roots::wire(&r.from, "close/pre/00-stray", "/somewhere/absolute");
    let refs = transfer(&[Object::Plugins], &r.from, &r.to).unwrap();
    assert!(!r.to.join("config/plugins/close/pre/00-stray").exists());
    assert!(refs.is_empty());
}

#[test]
fn a_shallow_or_non_entry_symlink_is_mirrored_but_not_recorded() {
    let r = Roots::new();
    Roots::wire(&r.from, "loose", "../../bin/x"); // one component: no op
    Roots::wire(&r.from, "close/pre/noentry", "../../bin/y"); // filename not NN-
    let refs = transfer(&[Object::Plugins], &r.from, &r.to).unwrap();
    assert_eq!(link_target(&r.to, "loose"), Path::new("../../bin/x"));
    assert_eq!(link_target(&r.to, "close/pre/noentry"), Path::new("../../bin/y"));
    assert!(refs.is_empty());
}

#[test]
fn plugins_replaces_an_existing_entry_innermost_wins() {
    let r = Roots::new();
    Roots::wire(&r.from, "close/pre/00-tracker", "../../bin/new");
    Roots::wire(&r.to, "close/pre/00-tracker", "../../bin/old");
    transfer(&[Object::Plugins], &r.from, &r.to).unwrap();
    assert_eq!(link_target(&r.to, "close/pre/00-tracker"), Path::new("../../bin/new"));
}

#[test]
fn an_absent_source_wiring_tree_copies_nothing() {
    let r = Roots::new();
    let refs = transfer(&[Object::Plugins], &r.from, &r.to).unwrap();
    assert!(refs.is_empty());
    assert!(!r.to.join("config/plugins").exists());
}

#[test]
fn config_replaces_balls_toml_and_skips_an_absent_source() {
    let r = Roots::new();
    transfer(&[Object::Config], &r.from, &r.to).unwrap(); // absent source: no-op
    assert!(!r.to.join("config/balls.toml").exists());

    Roots::file(&r.from, "config/balls.toml", "publish = true\n");
    Roots::file(&r.to, "config/balls.toml", "publish = false\n");
    transfer(&[Object::Config], &r.from, &r.to).unwrap();
    assert_eq!(fs::read_to_string(r.to.join("config/balls.toml")).unwrap(), "publish = true\n");
}

#[test]
fn tasks_unions_new_balls_and_ignores_non_md() {
    let r = Roots::new();
    Roots::file(&r.from, "tasks/bl-aaaa.md", "a");
    Roots::file(&r.from, "tasks/README", "not a ball");
    Roots::file(&r.to, "tasks/bl-bbbb.md", "b");
    transfer(&[Object::Tasks], &r.from, &r.to).unwrap();
    assert!(r.to.join("tasks/bl-aaaa.md").exists());
    assert!(r.to.join("tasks/bl-bbbb.md").exists()); // untouched
    assert!(!r.to.join("tasks/README").exists());
}

#[test]
fn tasks_refuses_to_clobber_a_same_id_on_both_sides() {
    let r = Roots::new();
    Roots::file(&r.from, "tasks/bl-dupe.md", "source");
    Roots::file(&r.to, "tasks/bl-dupe.md", "dest");
    let err = transfer(&[Object::Tasks], &r.from, &r.to).unwrap_err();
    assert!(matches!(&err, InstallError::Conflict { id } if id == "bl-dupe"));
    assert_eq!(fs::read_to_string(r.to.join("tasks/bl-dupe.md")).unwrap(), "dest");
}

#[test]
fn an_absent_source_tasks_dir_moves_nothing() {
    let r = Roots::new();
    transfer(&[Object::Tasks], &r.from, &r.to).unwrap();
    assert!(!r.to.join("tasks").exists());
}

#[test]
fn the_default_bundle_transfers_wiring_and_config_together() {
    let r = Roots::new();
    Roots::wire(&r.from, "close/pre/00-tracker", "../../bin/tracker");
    Roots::file(&r.from, "config/balls.toml", "x = 1\n");
    let refs = transfer(&Object::DEFAULT_BUNDLE, &r.from, &r.to).unwrap();
    assert_eq!(link_target(&r.to, "close/pre/00-tracker"), Path::new("../../bin/tracker"));
    assert!(r.to.join("config/balls.toml").exists());
    assert!(refs.contains_key("tracker"));
}

#[test]
fn install_error_displays_and_wraps_io() {
    let conflict = InstallError::Conflict { id: "bl-x".into() };
    assert!(conflict.to_string().contains("bl-x"));
    let io = InstallError::from(io::Error::new(io::ErrorKind::NotFound, "boom"));
    assert!(io.to_string().contains("boom"));
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
