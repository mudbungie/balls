//! Tests for §4 config resolution. `resolve` is driven against a throwaway
//! landing checkout (its `config/balls.toml`) plus an XDG user layer, so the
//! user-wins-over-landing ordering is exercised on real files; the merge helpers
//! are tested directly on `toml::Table`s so the list-compose directives are
//! covered without a built-in list field.

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

/// Write `body` to `<dir>/config/balls.toml`, creating the config dir, and
/// return the landing checkout path for [`EffectiveConfig::resolve`].
fn landing(tmp: &TempDir, name: &str, body: &str) -> PathBuf {
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
    let cfg = EffectiveConfig::resolve(&bare(&tmp, "landing"), &bare(&tmp, "absent.toml")).unwrap();
    assert_eq!(cfg, EffectiveConfig::default());
    assert_eq!(cfg.tasks_branch, DEFAULT_TASKS_BRANCH);
}

#[test]
fn the_landing_value_is_read() {
    let tmp = TempDir::new().unwrap();
    let land = landing(&tmp, "landing", "tasks_branch = \"land\"\n");
    let cfg = EffectiveConfig::resolve(&land, &bare(&tmp, "absent.toml")).unwrap();
    assert_eq!(cfg.tasks_branch, "land");
}

#[test]
fn log_level_defaults_to_info_and_reads_from_a_layer() {
    let tmp = TempDir::new().unwrap();
    // Absent ⇒ the serde-default scalar.
    let cfg = EffectiveConfig::resolve(&bare(&tmp, "none"), &bare(&tmp, "absent.toml")).unwrap();
    assert_eq!(cfg.log_level, "info");
    // Set on the landing ⇒ that value, like any §4 field.
    let land = landing(&tmp, "noisy", "log_level = \"debug\"\n");
    let cfg = EffectiveConfig::resolve(&land, &bare(&tmp, "absent.toml")).unwrap();
    assert_eq!(cfg.log_level, "debug");
}

#[test]
fn the_user_config_overrides_the_landing() {
    let tmp = TempDir::new().unwrap();
    let land = landing(&tmp, "landing", "tasks_branch = \"land\"\n");
    let user = tmp.path().join("user.toml");
    fs::write(&user, "tasks_branch = \"user\"\n").unwrap();
    let cfg = EffectiveConfig::resolve(&land, &user).unwrap();
    assert_eq!(cfg.tasks_branch, "user");
}

#[test]
fn a_malformed_layer_is_an_error_naming_the_file() {
    let tmp = TempDir::new().unwrap();
    let land = landing(&tmp, "landing", "tasks_branch = [not valid toml\n");
    let err = EffectiveConfig::resolve(&land, &bare(&tmp, "absent.toml")).unwrap_err();
    assert!(err.to_string().contains("balls.toml"));
}

#[test]
fn a_layer_read_error_other_than_absence_propagates() {
    let tmp = TempDir::new().unwrap();
    // The user-config path is a directory: read_to_string errors, not NotFound.
    let dir = tmp.path().join("a-dir");
    fs::create_dir_all(&dir).unwrap();
    assert!(EffectiveConfig::resolve(&bare(&tmp, "landing"), &dir).is_err());
}

#[test]
fn a_tasks_branch_naming_the_landing_is_refused_by_name() {
    // §2/§4 (bl-ac89): one branch cannot back two checkouts of one repo, and the
    // landing is single-owner — never a store. The read authority refuses the
    // coincident name NAMED, whichever layer carries it, so a seeded or
    // hand-edited poison fails on every op instead of wedging prime on a git fatal.
    let tmp = TempDir::new().unwrap();
    let land = landing(&tmp, "landing", "tasks_branch = \"balls/config\"\n");
    let err = EffectiveConfig::resolve(&land, &bare(&tmp, "absent.toml")).unwrap_err();
    assert!(err.to_string().contains("names the landing"), "{err}");
    // The user layer is the same one check — it wins the merge, then is refused.
    let user = tmp.path().join("user.toml");
    fs::write(&user, "tasks_branch = \"balls/config\"\n").unwrap();
    let err = EffectiveConfig::resolve(&bare(&tmp, "clean"), &user).unwrap_err();
    assert!(err.to_string().contains("names the landing"), "{err}");
}

#[test]
fn a_value_that_clashes_with_the_typed_schema_does_not_resolve() {
    let tmp = TempDir::new().unwrap();
    // `tasks_branch` typed as a string, given an array — projection fails.
    let land = landing(&tmp, "landing", "tasks_branch = [1, 2]\n");
    let err = EffectiveConfig::resolve(&land, &bare(&tmp, "absent.toml")).unwrap_err();
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
    // The plain `tasks_branch` key takes the insert arm; `_append` the directive.
    let mut base = table("tasks_branch = 'old'\ntags = ['a']");
    layer_over(&mut base, table("tasks_branch = 'new'\ntags_append = ['b']"));
    assert_eq!(base["tasks_branch"].as_str(), Some("new"));
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

#[test]
fn xdg_remote_reads_the_user_layer_remote_else_none() {
    // The §12 EXPLICIT remote tier core reads (over which the tracker discovers
    // `origin`): the per-machine XDG user layer's `remote` key, or `None`.
    let tmp = TempDir::new().unwrap();
    // A user layer naming a remote → Some.
    let with = tmp.path().join("with.toml");
    fs::write(&with, "remote = \"git@hub:xdg\"\n").unwrap();
    assert_eq!(xdg_remote(&with).as_deref(), Some("git@hub:xdg"));
    // A user layer present but with no `remote` key → None.
    let without = tmp.path().join("without.toml");
    fs::write(&without, "log_level = \"info\"\n").unwrap();
    assert_eq!(xdg_remote(&without), None);
    // No user layer file at all → None.
    assert_eq!(xdg_remote(&tmp.path().join("absent.toml")), None);
}

#[test]
fn remote_ladder_resolves_cli_over_sentinel_over_xdg() {
    // §12/bl-9df0: per-op flag > landing `task_remote` (the stealth sentinel —
    // resolution STOPS above origin) > XDG `remote`.
    let tmp = TempDir::new().unwrap();
    let user = tmp.path().join("config.toml");
    fs::write(&user, "remote = \"git@hub:xdg\"\n").unwrap();
    // No landing rung → the XDG tier, not declared.
    let empty = landing(&tmp, "empty", "");
    assert_eq!(remote_ladder(None, &empty, &user).unwrap(), (Some("git@hub:xdg".into()), false));
    // The sentinel declares stealth and stops resolution, XDG unread.
    let stealth = landing(&tmp, "stealth", "task_remote = \"none\"\n");
    assert_eq!(remote_ladder(None, &stealth, &user).unwrap(), (None, true));
    // Consent given supersedes withheld — the per-op tier outranks the sentinel.
    assert_eq!(
        remote_ladder(Some("git@hub:op".into()), &stealth, &user).unwrap(),
        (Some("git@hub:op".into()), false)
    );
}

#[test]
fn remote_ladder_refuses_a_url_in_the_landing_rung() {
    // A URL's home is per-machine (§4/§12 — it must never travel on `install`,
    // and an installed config silently redirecting your store pushes is exactly
    // the surprise that rule prevents); the landing rung holds only the
    // sentinel, anything else refused NAMED.
    let tmp = TempDir::new().unwrap();
    let poison = landing(&tmp, "poison", "task_remote = \"git@hub:evil\"\n");
    let err = remote_ladder(None, &poison, &tmp.path().join("absent.toml")).unwrap_err().to_string();
    assert!(err.contains("per-machine"), "{err}");
}
