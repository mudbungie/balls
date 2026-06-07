//! Tests for the §6 `[hooks]` schedule — parse, the dispatch resolution that
//! stitches names to local binaries, the install-time referenced projection, and
//! the seed's prune + re-serialize round-trip.

use super::*;
use std::collections::BTreeSet;
use std::fs;
use tempfile::TempDir;

const SAMPLE: &str = r#"
[hooks]
"close.pre"  = ["bl-delivery"]
"close.post" = ["bl-delivery", "tracker"]
"create.post" = ["tracker"]
"#;

#[test]
fn parse_reads_each_op_phase_list_in_order() {
    let hooks = Hooks::parse(SAMPLE).unwrap();
    assert_eq!(hooks.names("close", "post"), ["bl-delivery", "tracker"]);
    assert_eq!(hooks.names("create", "post"), ["tracker"]);
    // An un-wired op-phase is the empty list (run nothing).
    assert!(hooks.names("drop", "post").is_empty());
}

#[test]
fn parse_tolerates_a_missing_hooks_table_and_non_string_entries() {
    // No [hooks] table at all → an empty schedule, not an error.
    assert!(Hooks::parse("tasks_branch = \"x\"\n").unwrap().names("close", "pre").is_empty());
    // A non-string array element is filtered out; the rest survive.
    let hooks = Hooks::parse("[hooks]\n\"close.pre\" = [\"tracker\", 7]\n").unwrap();
    assert_eq!(hooks.names("close", "pre"), ["tracker"]);
}

#[test]
fn parse_rejects_malformed_toml() {
    assert!(Hooks::parse("[hooks").is_err());
}

#[test]
fn load_from_absent_file_is_an_empty_schedule() {
    let tmp = TempDir::new().unwrap();
    let hooks = Hooks::load_from(&tmp.path().join("nope.toml")).unwrap();
    assert_eq!(hooks, Hooks::default());
}

#[test]
fn load_from_propagates_a_non_not_found_read_error() {
    // Reading a directory as a file errors with a kind other than NotFound — that
    // is surfaced, not swallowed (only an absent file is the empty case).
    let tmp = TempDir::new().unwrap();
    assert!(Hooks::load_from(tmp.path()).is_err());
}

#[test]
fn load_reads_the_landing_plugins_toml() {
    let tmp = TempDir::new().unwrap();
    let config = tmp.path().join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("plugins.toml"), SAMPLE).unwrap();
    assert_eq!(Hooks::load(tmp.path()).unwrap().names("create", "post"), ["tracker"]);
}

#[test]
fn resolve_stitches_names_to_local_binaries_in_order() {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    let bin = tmp.path().join("real");
    fs::write(&bin, "x").unwrap();
    reg.bind("tracker", &bin).unwrap(); // only tracker is installed here
    let hooks = Hooks::parse(SAMPLE).unwrap();

    let refs = hooks.resolve(&reg, "close", "post");
    assert_eq!(refs.len(), 2);
    // List order preserved: bl-delivery first (dangling — not bound), tracker last.
    assert_eq!(refs[0].name, "bl-delivery");
    assert_eq!(refs[0].bin, None);
    assert_eq!(refs[1].name, "tracker");
    assert_eq!(refs[1].bin, Some(fs::canonicalize(&bin).unwrap()));
}

#[test]
fn referenced_maps_each_plugin_to_the_ops_it_is_wired_into() {
    let refs = Hooks::parse(SAMPLE).unwrap().referenced();
    assert_eq!(refs["tracker"], BTreeSet::from(["close".to_string(), "create".to_string()]));
    assert_eq!(refs["bl-delivery"], BTreeSet::from(["close".to_string()]));
}

#[test]
fn retain_drops_names_failing_the_predicate() {
    let mut hooks = Hooks::parse(SAMPLE).unwrap();
    hooks.retain(|name| name == "tracker"); // prune the absent bl-delivery
    assert_eq!(hooks.names("close", "post"), ["tracker"]);
    assert!(hooks.names("close", "pre").is_empty()); // only bl-delivery → emptied
}

#[test]
fn to_toml_round_trips_through_parse_and_drops_emptied_lists() {
    let mut hooks = Hooks::parse(SAMPLE).unwrap();
    hooks.retain(|name| name == "tracker");
    let body = hooks.to_toml();
    let reparsed = Hooks::parse(&body).unwrap();
    assert_eq!(reparsed.names("close", "post"), ["tracker"]);
    // An entry whose whole list pruned away is gone, not an empty array.
    assert!(!body.contains("close.pre"));
}
