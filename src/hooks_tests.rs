//! Tests for the §6 `[hooks]` schedule — parse, the dispatch resolution that
//! stitches names to local binaries, the install-time referenced projection, and
//! the seed's prune + re-serialize round-trip.

use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

const SAMPLE: &str = r#"
[hooks]
"close.pre"  = ["bl-delivery"]
"close.post" = ["bl-delivery", "tracker"]
"create.post" = ["tracker"]
"show" = ["bl-delivery"]
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

/// Lay a landing `config/plugins.toml` and (optionally) an XDG `plugins.toml`
/// beside a `config.toml`, returning (landing root, user_config path) for
/// [`Hooks::effective`]. The XDG body is written only when `xdg` is `Some`.
fn layered(landing_body: &str, xdg: Option<&str>) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let config = tmp.path().join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("plugins.toml"), landing_body).unwrap();
    let xdg_dir = tmp.path().join("xdg");
    fs::create_dir_all(&xdg_dir).unwrap();
    if let Some(body) = xdg {
        fs::write(xdg_dir.join("plugins.toml"), body).unwrap();
    }
    let user_config = xdg_dir.join("config.toml"); // sibling of the XDG plugins.toml
    (tmp, user_config)
}

#[test]
fn effective_with_no_xdg_layer_is_the_landing_schedule() {
    let (tmp, user_config) = layered(SAMPLE, None);
    let hooks = Hooks::effective(tmp.path(), &user_config).unwrap();
    assert_eq!(hooks.names("close", "post"), ["bl-delivery", "tracker"]);
}

#[test]
fn effective_xdg_bare_key_replaces_and_directives_compose() {
    // XDG fully REPLACES close.post; APPENDS to create.post; PREPENDS the push-free
    // run before close.pre; BANS bl-delivery from close.post's replacement is moot
    // (replaced), so ban targets create.post instead.
    let xdg = r#"
[hooks]
"close.post" = ["only-me"]
"create.post_append" = ["mirror"]
"close.pre_prepend" = ["lint"]
"#;
    let (tmp, user_config) = layered(SAMPLE, Some(xdg));
    let hooks = Hooks::effective(tmp.path(), &user_config).unwrap();
    assert_eq!(hooks.names("close", "post"), ["only-me"], "bare key replaces");
    assert_eq!(hooks.names("create", "post"), ["tracker", "mirror"], "_append composes");
    assert_eq!(hooks.names("close", "pre"), ["lint", "bl-delivery"], "_prepend composes");
}

#[test]
fn effective_xdg_ban_removes_a_landing_entry() {
    let xdg = "[hooks]\n\"close.post_ban\" = [\"bl-delivery\"]\n";
    let (tmp, user_config) = layered(SAMPLE, Some(xdg));
    let hooks = Hooks::effective(tmp.path(), &user_config).unwrap();
    assert_eq!(hooks.names("close", "post"), ["tracker"], "_ban drops the named entry");
}

#[test]
fn effective_seeds_from_xdg_when_the_landing_has_none() {
    // No landing plugins.toml at all → the XDG layer alone seeds the schedule.
    let tmp = TempDir::new().unwrap();
    let xdg_dir = tmp.path().join("xdg");
    fs::create_dir_all(&xdg_dir).unwrap();
    fs::write(xdg_dir.join("plugins.toml"), "[hooks]\n\"sync.pre\" = [\"tracker\"]\n").unwrap();
    let hooks = Hooks::effective(tmp.path(), &xdg_dir.join("config.toml")).unwrap();
    assert_eq!(hooks.names("sync", "pre"), ["tracker"]);
}

#[test]
fn effective_treats_a_layer_without_a_hooks_table_as_empty() {
    // A present landing plugins.toml with no [hooks] contributes nothing (the
    // present-but-empty branch, distinct from an absent file); the XDG schedule stands.
    let (tmp, user_config) = layered("# nothing wired here\n", Some("[hooks]\n\"sync.pre\" = [\"tracker\"]\n"));
    let hooks = Hooks::effective(tmp.path(), &user_config).unwrap();
    assert_eq!(hooks.names("sync", "pre"), ["tracker"]);
    assert!(hooks.names("close", "post").is_empty());
}

#[test]
fn effective_rejects_a_malformed_layer() {
    let (tmp, user_config) = layered("[hooks", None);
    assert!(Hooks::effective(tmp.path(), &user_config).is_err());
}

#[test]
fn effective_propagates_a_non_not_found_read_error() {
    // A directory where a layer file is expected errors with a kind other than
    // NotFound — surfaced, not swallowed (only an absent file is the empty case).
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("config").join("plugins.toml")).unwrap();
    assert!(Hooks::effective(tmp.path(), &tmp.path().join("xdg").join("config.toml")).is_err());
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
fn resolve_read_uses_the_bare_op_key_for_the_single_read_phase() {
    // §6 read dispatch: a read has no pre/post split, so its hook key is the
    // bare op token; a phased key never answers it (and vice versa).
    let tmp = TempDir::new().unwrap();
    let reg = Registry::at(tmp.path());
    let hooks = Hooks::parse(SAMPLE).unwrap();
    let refs = hooks.resolve_read(&reg, "show");
    assert_eq!(refs.len(), 1);
    assert_eq!((refs[0].name.as_str(), refs[0].bin.as_deref()), ("bl-delivery", None));
    assert!(hooks.resolve_read(&reg, "close").is_empty()); // phased keys don't leak in
    assert!(hooks.names("show", "read").is_empty()); // nor the bare key out
}

#[test]
fn referenced_maps_each_plugin_to_the_ops_it_is_wired_into() {
    // A bare read key ("show") projects its op like any phased key does.
    let refs = Hooks::parse(SAMPLE).unwrap().referenced();
    assert_eq!(refs["tracker"], BTreeSet::from(["close".to_string(), "create".to_string()]));
    assert_eq!(refs["bl-delivery"], BTreeSet::from(["close".to_string(), "show".to_string()]));
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
