//! `bl-chore`'s PURE policy tests — the mint render (the whole safety property
//! of the data-not-shell schema) and the three readers it composes with: the
//! epic-skip predicate, the `bl-id` lookup, and the config load. No `bl` seam
//! and no wire: the dispatch/guard/mint/rollback tests live in the sibling
//! `chore_tests.rs`, which needs both.

use super::*;
use tempfile::TempDir;

#[test]
fn render_minimal_injects_the_gate_edge_tag_and_title_after_the_separator() {
    let spec = ChoreSpec { title: "Run the test suite".into(), body: None, priority: None };
    let argv = render_create(&spec, "bl-7a5f", "bl-chore", "me");
    assert_eq!(
        argv,
        vec![
            "create", "--parent", "bl-7a5f", "--blocks", "close", "-t", "bl-chore", "--as", "me", "--",
            "Run the test suite"
        ]
    );
}

#[test]
fn render_threads_priority_and_body_before_the_separator() {
    let spec = ChoreSpec { title: "Docs".into(), body: Some("check §6".into()), priority: Some(1) };
    let argv = render_create(&spec, "bl-1", "bl-chore", "me");
    let dd = argv.iter().position(|a| a == "--").unwrap();
    assert_eq!(argv.last().unwrap(), "Docs");
    assert!(argv[..dd].contains(&"-p".to_string()) && argv[..dd].contains(&"1".to_string()));
    assert!(argv[..dd].contains(&"--body".to_string()) && argv[..dd].contains(&"check §6".to_string()));
}

#[test]
fn a_flag_like_title_stays_the_lone_trailing_positional() {
    // The headline Option-A safety property: a hostile title is inert data, the
    // single positional after `--` — never parsed as a flag (design bl-3df3).
    let spec = ChoreSpec { title: "--blocks close -t evil".into(), body: None, priority: None };
    let argv = render_create(&spec, "bl-1", "bl-chore", "me");
    let dd = argv.iter().position(|a| a == "--").unwrap();
    assert_eq!(argv.last().unwrap(), "--blocks close -t evil");
    assert_eq!(argv.iter().filter(|a| *a == "--blocks close -t evil").count(), 1);
    assert!(argv.iter().rposition(|a| a == "--blocks close -t evil").unwrap() > dd);
}

#[test]
fn has_children_sees_a_matching_parent_only() {
    let json = r#"[{"parent":"bl-1"},{"parent":"bl-2"},{}]"#;
    assert!(has_children(json, "bl-1").unwrap());
    assert!(!has_children(json, "bl-9").unwrap());
    assert!(has_children("not json", "bl-1").is_err());
}

#[test]
fn claimed_id_reads_the_first_bl_id_else_errors() {
    let mut md = BTreeMap::new();
    md.insert("bl-id".to_string(), vec!["bl-42".to_string()]);
    assert_eq!(claimed_id(&md).unwrap(), "bl-42");
    assert!(claimed_id(&BTreeMap::new()).is_err());
}

#[test]
fn config_path_is_the_plugins_own_landing_territory() {
    assert_eq!(config_path("/land", "bl-chore"), Path::new("/land/config/plugins/bl-chore/chores.toml"));
}

#[test]
fn load_config_absent_is_empty_present_parses_garbage_and_a_dir_error() {
    let tmp = TempDir::new().unwrap();
    let absent = tmp.path().join("nope.toml");
    let c = load_config(&absent).unwrap();
    assert!(c.epic_skip && c.chore.is_empty());

    let good = tmp.path().join("good.toml");
    fs::write(&good, "epic_skip = false\n[[chore]]\ntitle = \"x\"\n").unwrap();
    let c = load_config(&good).unwrap();
    assert!(!c.epic_skip && c.chore.len() == 1);

    let bad = tmp.path().join("bad.toml");
    fs::write(&bad, "title = [unclosed").unwrap();
    assert!(load_config(&bad).is_err());

    // A path that exists but is a directory: read_to_string errors non-NotFound.
    assert!(load_config(tmp.path()).is_err());
}
