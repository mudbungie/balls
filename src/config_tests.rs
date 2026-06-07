//! Tests for §4 config layering. `resolve` is driven against throwaway checkout
//! dirs (one per trail node) so the innermost(landing)-wins ordering is exercised
//! on real files; the merge helpers are tested directly on `toml::Table`s so the
//! list-compose directives are covered without a built-in list field.

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

/// Write `body` to `<dir>/config/balls.toml`, creating the config dir, and
/// return the checkout path — one trail node for [`EffectiveConfig::resolve`].
fn checkout(tmp: &TempDir, name: &str, body: &str) -> PathBuf {
    let dir = tmp.path().join(name);
    fs::create_dir_all(dir.join("config")).unwrap();
    fs::write(dir.join("config").join("balls.toml"), body).unwrap();
    dir
}

/// A path with no `config/balls.toml` — an empty layer.
fn bare(tmp: &TempDir, name: &str) -> PathBuf {
    tmp.path().join(name)
}

/// Parse a TOML table from `body` — the layer/base shape the merge helpers take.
fn table(body: &str) -> Table {
    toml::from_str(body).unwrap()
}

#[test]
fn with_no_layers_it_resolves_to_the_built_in_defaults() {
    let tmp = TempDir::new().unwrap();
    let trail = [bare(&tmp, "landing")];
    let cfg = EffectiveConfig::resolve(&trail, &bare(&tmp, "absent.toml")).unwrap();
    assert_eq!(cfg, EffectiveConfig::default());
    assert_eq!(cfg.branch, STATE_BRANCH);
}

#[test]
fn the_landing_value_wins_over_a_downstream_trail_step() {
    let tmp = TempDir::new().unwrap();
    // Trail is landing-first; the terminus is the outermost (lowest) layer.
    let trail = [
        checkout(&tmp, "landing", "branch = \"land\"\n"),
        checkout(&tmp, "terminus", "branch = \"term\"\n"),
    ];
    let cfg = EffectiveConfig::resolve(&trail, &bare(&tmp, "absent.toml")).unwrap();
    assert_eq!(cfg.branch, "land");
}

#[test]
fn the_user_config_overrides_the_landing() {
    let tmp = TempDir::new().unwrap();
    let trail = [checkout(&tmp, "landing", "branch = \"land\"\n")];
    let user = tmp.path().join("user.toml");
    fs::write(&user, "branch = \"user\"\n").unwrap();
    let cfg = EffectiveConfig::resolve(&trail, &user).unwrap();
    assert_eq!(cfg.branch, "user");
}

#[test]
fn a_malformed_layer_is_an_error_naming_the_file() {
    let tmp = TempDir::new().unwrap();
    let trail = [checkout(&tmp, "landing", "branch = [not valid toml\n")];
    let err = EffectiveConfig::resolve(&trail, &bare(&tmp, "absent.toml")).unwrap_err();
    assert!(err.to_string().contains("balls.toml"));
}

#[test]
fn a_layer_read_error_other_than_absence_propagates() {
    let tmp = TempDir::new().unwrap();
    // The user-config path is a directory: read_to_string errors, not NotFound.
    let dir = tmp.path().join("a-dir");
    fs::create_dir_all(&dir).unwrap();
    assert!(EffectiveConfig::resolve(&[bare(&tmp, "landing")], &dir).is_err());
}

#[test]
fn a_value_that_clashes_with_the_typed_schema_does_not_resolve() {
    let tmp = TempDir::new().unwrap();
    // `branch` typed as a string, given an array — projection fails.
    let trail = [checkout(&tmp, "landing", "branch = [1, 2]\n")];
    let err = EffectiveConfig::resolve(&trail, &bare(&tmp, "absent.toml")).unwrap_err();
    assert!(err.to_string().contains("config does not resolve"));
}

#[test]
fn list_append_concatenates_incoming_after_the_current() {
    let mut base = table("tags = ['a']");
    layer_over(&mut base, table("tags_append = ['b']"));
    assert_eq!(base["tags"].as_array().unwrap().len(), 2);
    assert_eq!(base["tags"][0].as_str(), Some("a"));
    assert_eq!(base["tags"][1].as_str(), Some("b"));
}

#[test]
fn list_prepend_puts_incoming_first() {
    let mut base = table("tags = ['b']");
    layer_over(&mut base, table("tags_prepend = ['a']"));
    assert_eq!(base["tags"][0].as_str(), Some("a"));
    assert_eq!(base["tags"][1].as_str(), Some("b"));
}

#[test]
fn list_ban_removes_every_named_element() {
    let mut base = table("tags = ['a', 'b', 'c']");
    layer_over(&mut base, table("tags_ban = ['b']"));
    let tags: Vec<_> = base["tags"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert_eq!(tags, ["a", "c"]);
}

#[test]
fn a_bare_field_replaces_and_a_directive_composes_in_one_layer() {
    // The plain `branch` key takes the insert arm; the `_append` the directive arm.
    let mut base = table("branch = 'old'\ntags = ['a']");
    layer_over(&mut base, table("branch = 'new'\ntags_append = ['b']"));
    assert_eq!(base["branch"].as_str(), Some("new"));
    assert_eq!(base["tags"].as_array().unwrap().len(), 2);
}

#[test]
fn a_directive_seeds_an_absent_field() {
    let mut base = Table::new();
    layer_over(&mut base, table("tags_append = ['a']"));
    assert_eq!(base["tags"].as_array().unwrap().len(), 1);
}

#[test]
fn a_directive_over_a_non_list_treats_the_current_as_empty() {
    let mut base = table("tags = 'scalar'");
    layer_over(&mut base, table("tags_append = ['a']"));
    let tags = base["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].as_str(), Some("a"));
}

#[test]
fn list_directive_splits_known_suffixes_only() {
    assert!(matches!(list_directive("tags_prepend"), Some(("tags", ListOp::Prepend))));
    assert!(matches!(list_directive("tags_append"), Some(("tags", ListOp::Append))));
    assert!(matches!(list_directive("tags_ban"), Some(("tags", ListOp::Ban))));
    assert!(list_directive("tags").is_none()); // plain key
    assert!(list_directive("_append").is_none()); // bare suffix, empty field
}
