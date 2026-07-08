//! Tests for §12.1 rename convergence (bl-18bf): prime rewrites a retired
//! first-party plugin name in the landing schedule and binds the current name,
//! guarded by the live-binding check, landing-only, converging on a repeat.

#![cfg(unix)]

use crate::converge;
use crate::git;
use crate::layout::Xdg;
use crate::registry::Registry;
use crate::substrate;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// An XDG rooted in `tmp` (its own state dir), enough to found a landing.
fn xdg(tmp: &TempDir) -> Xdg {
    Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy()))
}

/// A founded landing whose committed `config/plugins.toml` is overwritten with
/// `body` and re-sealed — a version-skewed checkout as an old binary left it.
fn landing_with(tmp: &TempDir, body: &str) -> PathBuf {
    let landing = tmp.path().join("landing");
    substrate::found_landing(&landing, &xdg(tmp), None, "tester").unwrap();
    fs::write(landing.join("config/plugins.toml"), body).unwrap();
    git::run(&landing, &["add", "-A"], None).unwrap();
    git::run(&landing, &["commit", "-q", "-m", "stale"], None).unwrap();
    landing
}

/// An `exe_dir` shipping a `bl-tracker` sibling beside `bl` (content irrelevant —
/// [`crate::seed::sibling`] only tests existence).
fn exe_dir(tmp: &TempDir) -> PathBuf {
    let dir = tmp.path().join("exe");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bl-tracker"), "#!/bin/sh\n").unwrap();
    dir
}

fn commits(landing: &Path) -> usize {
    git::run(landing, &["rev-list", "--count", "HEAD"], None).unwrap().trim().parse().unwrap()
}

fn plugins_toml(landing: &Path) -> toml::Table {
    toml::from_str(&fs::read_to_string(landing.join("config/plugins.toml")).unwrap()).unwrap()
}

fn bin(landing: &Path, name: &str) -> PathBuf {
    landing.join("config/plugins/bin").join(name)
}

#[test]
fn rewrites_the_hooks_schedule_binds_the_current_name_and_drops_the_dangling_old_symlink() {
    let tmp = TempDir::new().unwrap();
    let exe = exe_dir(&tmp);
    let landing = landing_with(&tmp, "[hooks]\n\"prime.pre\" = [\"bl-delivery\", \"tracker\"]\n");
    // An old prime left a dangling bin/tracker (its binary is gone) — a bare
    // rewrite would turn its skip-with-notice into a hard abort.
    fs::create_dir_all(bin(&landing, "")).unwrap();
    symlink(tmp.path().join("gone"), bin(&landing, "tracker")).unwrap();
    let before = commits(&landing);

    converge::converge(&landing, Some(&exe), "tester").unwrap();

    let hooks = plugins_toml(&landing)["hooks"]["prime.pre"].as_array().unwrap().clone();
    let names: Vec<&str> = hooks.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(names, ["bl-delivery", "bl-tracker"], "the retired name is rewritten, the rest kept");
    assert_eq!(commits(&landing), before + 1, "one landing commit");
    assert_eq!(git::run(&landing, &["log", "-1", "--format=%s"], None).unwrap().trim(), "balls: converge tracker->bl-tracker");
    assert_eq!(fs::read_link(bin(&landing, "bl-tracker")).unwrap(), exe.join("bl-tracker"), "current name bound to its sibling");
    assert!(bin(&landing, "tracker").symlink_metadata().is_err(), "the dangling old symlink is dropped");
}

#[test]
fn a_second_prime_is_a_no_op_no_commit() {
    let tmp = TempDir::new().unwrap();
    let exe = exe_dir(&tmp);
    let landing = landing_with(&tmp, "[hooks]\n\"prime.pre\" = [\"tracker\"]\n");
    converge::converge(&landing, Some(&exe), "tester").unwrap();
    let converged = commits(&landing);
    converge::converge(&landing, Some(&exe), "tester").unwrap();
    assert_eq!(commits(&landing), converged, "the converged checkout re-primes to a no-op");
}

#[test]
fn a_converged_landing_no_ops_from_the_start() {
    // A freshly-founded landing already names bl-tracker (the embedded seed): the
    // first-prime / already-converged path is a clean no-op (no git, no bind).
    let tmp = TempDir::new().unwrap();
    let landing = tmp.path().join("landing");
    substrate::found_landing(&landing, &xdg(&tmp), None, "tester").unwrap();
    let before = commits(&landing);
    converge::converge(&landing, None, "tester").unwrap();
    assert_eq!(commits(&landing), before);
}

#[test]
fn rewrites_a_source_key_and_round_trips_foreign_and_unrelated_tables() {
    let tmp = TempDir::new().unwrap();
    let landing = landing_with(
        &tmp,
        "[hooks]\n\"prime.pre\" = [\"tracker\"]\n\n[source]\nbl-delivery = \"cargo install balls-delivery\"\n\n[team]\nowner = \"acme\"\n",
    );
    converge::converge(&landing, Some(&exe_dir(&tmp)), "tester").unwrap();
    let root = plugins_toml(&landing);
    assert_eq!(root["hooks"]["prime.pre"][0].as_str().unwrap(), "bl-tracker");
    // A [source] key present for a NON-retired name is left whole (the remove
    // finds no `tracker` key), and a team's foreign [team] table round-trips.
    let source = root["source"].as_table().unwrap();
    assert!(source.contains_key("bl-delivery") && !source.contains_key("tracker"));
    assert_eq!(root["team"]["owner"].as_str().unwrap(), "acme");
}

#[test]
fn rewrites_a_retired_source_key_carrying_its_hint() {
    let tmp = TempDir::new().unwrap();
    let landing = landing_with(&tmp, "[hooks]\n\n[source]\ntracker = \"git clone x && make install\"\n");
    converge::converge(&landing, Some(&exe_dir(&tmp)), "tester").unwrap();
    let root = plugins_toml(&landing);
    let source = root["source"].as_table().unwrap();
    assert!(!source.contains_key("tracker"), "the retired [source] key is re-keyed");
    assert_eq!(source["bl-tracker"].as_str().unwrap(), "git clone x && make install", "the hint is carried verbatim");
}

#[test]
fn a_live_bound_old_name_is_a_third_party_plugin_left_untouched() {
    let tmp = TempDir::new().unwrap();
    let landing = landing_with(&tmp, "[hooks]\n\"prime.pre\" = [\"tracker\"]\n");
    // A third party legitimately ships a LIVE-bound `tracker` (not reserved).
    fs::create_dir_all(bin(&landing, "")).unwrap();
    let real = tmp.path().join("third-party-tracker");
    fs::write(&real, "#!/bin/sh\n").unwrap();
    symlink(&real, bin(&landing, "tracker")).unwrap();
    let before = commits(&landing);

    converge::converge(&landing, Some(&exe_dir(&tmp)), "tester").unwrap();

    assert_eq!(plugins_toml(&landing)["hooks"]["prime.pre"][0].as_str().unwrap(), "tracker", "the name is left whole");
    assert_eq!(commits(&landing), before, "no rewrite, no commit");
    assert!(bin(&landing, "tracker").symlink_metadata().is_ok(), "the live binding survives");
}

#[test]
fn a_sibling_absent_leaves_the_current_name_unbound() {
    // No `bl-tracker` beside `bl` (exe_dir None): the rewrite still lands, but the
    // current name is left unbound — the ordinary [source]-hinted refusal covers
    // it and `bl install` binds it, exactly as on a fresh machine.
    let tmp = TempDir::new().unwrap();
    let landing = landing_with(&tmp, "[hooks]\n\"prime.pre\" = [\"tracker\"]\n");
    converge::converge(&landing, None, "tester").unwrap();
    assert_eq!(plugins_toml(&landing)["hooks"]["prime.pre"][0].as_str().unwrap(), "bl-tracker");
    assert!(bin(&landing, "bl-tracker").symlink_metadata().is_err(), "no sibling ⇒ current name unbound");
}

#[test]
fn the_xdg_layer_is_never_edited() {
    // Convergence is LANDING-only (the dispatch notice is the XDG layer's cover):
    // an XDG plugins.toml naming the retired name is left byte-identical.
    let tmp = TempDir::new().unwrap();
    let x = xdg(&tmp);
    let user = x.user_config().with_file_name("plugins.toml");
    fs::create_dir_all(user.parent().unwrap()).unwrap();
    let body = "[hooks]\n\"prime.pre\" = [\"tracker\"]\n";
    fs::write(&user, body).unwrap();
    let landing = landing_with(&tmp, "[hooks]\n\"prime.pre\" = [\"tracker\"]\n");
    converge::converge(&landing, Some(&exe_dir(&tmp)), "tester").unwrap();
    assert_eq!(fs::read_to_string(&user).unwrap(), body, "the machine layer is untouched");
}

#[test]
fn rewrite_config_canonicalizes_an_adopt_copy_in_under_the_guard() {
    // The adopt copy-in transform (before the seal): a retired-and-unbound name in
    // the staged config is rewritten; the landing's registry is the guard.
    let tmp = TempDir::new().unwrap();
    let change = tmp.path().join("change");
    fs::create_dir_all(change.join("config")).unwrap();
    fs::write(change.join("config/plugins.toml"), "[hooks]\n\"install.pre\" = [\"tracker\"]\n").unwrap();
    let reg = Registry::at(&tmp.path().join("landing")); // no bin/ ⇒ tracker unbound
    converge::rewrite_config(&change, &reg).unwrap();
    let root: toml::Table = toml::from_str(&fs::read_to_string(change.join("config/plugins.toml")).unwrap()).unwrap();
    assert_eq!(root["hooks"]["install.pre"][0].as_str().unwrap(), "bl-tracker");
}

#[test]
fn rewrite_config_leaves_a_clean_copy_in_byte_identical() {
    // No retired name ⇒ the copied bytes are NOT reserialized — comments and
    // formatting survive, so identical adopted config still seals to nothing.
    let tmp = TempDir::new().unwrap();
    let change = tmp.path().join("change");
    fs::create_dir_all(change.join("config")).unwrap();
    let body = "# team schedule — keep me\n[hooks]\n\"install.pre\" = [\"bl-tracker\"]\n";
    fs::write(change.join("config/plugins.toml"), body).unwrap();
    let reg = Registry::at(&tmp.path().join("landing"));
    converge::rewrite_config(&change, &reg).unwrap();
    assert_eq!(fs::read_to_string(change.join("config/plugins.toml")).unwrap(), body);
    // A copy-in that carries no schedule at all is a clean no-op too.
    let bare = tmp.path().join("bare");
    fs::create_dir_all(&bare).unwrap();
    converge::rewrite_config(&bare, &reg).unwrap();
}
