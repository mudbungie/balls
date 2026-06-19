//! `bl-chore` policy tests — every guard, the mint render, and the helpers,
//! against a fake [`Bl`] (no real `bl`) and temp config files.

use super::*;
use std::cell::RefCell;
use tempfile::TempDir;

/// A fake [`Bl`] recording every call; `list` returns a scripted JSON, `create`
/// succeeds unless `create_fails`.
struct FakeBl {
    list_json: String,
    create_fails: bool,
    calls: RefCell<Vec<Vec<String>>>,
}

impl FakeBl {
    fn new(list_json: &str) -> Self {
        Self { list_json: list_json.into(), create_fails: false, calls: RefCell::new(Vec::new()) }
    }
    fn creates(&self) -> Vec<Vec<String>> {
        self.calls.borrow().iter().filter(|a| a.first().map(String::as_str) == Some("create")).cloned().collect()
    }
    fn listed(&self) -> bool {
        self.calls.borrow().iter().any(|a| a.first().map(String::as_str) == Some("list"))
    }
}

impl Bl for FakeBl {
    fn run(&self, _cwd: &Path, argv: &[String]) -> io::Result<String> {
        self.calls.borrow_mut().push(argv.to_vec());
        if argv.first().map(String::as_str) == Some("list") {
            return Ok(self.list_json.clone());
        }
        if self.create_fails {
            return Err(io::Error::other("boom"));
        }
        Ok(String::new())
    }
}

/// A temp landing whose `config/plugins/bl-chore/chores.toml` holds `toml`.
fn landing_with(toml: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("config/plugins/bl-chore");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("chores.toml"), toml).unwrap();
    tmp
}

/// A claim.post wire JSON with the given landing, tags, and `bl-id`.
fn wire(landing: &str, tags: &[&str], bl_id: Option<&str>) -> String {
    let mut v = serde_json::json!({
        "actor": "tester",
        "binding": { "landing": landing, "invocation_path": "/proj" },
        "previous_state": { "tags": tags },
    });
    if let Some(id) = bl_id {
        v["metadata"] = serde_json::json!({ "bl-id": [id] });
    }
    v.to_string()
}

const TWO_CHORES: &str = "[[chore]]\ntitle = \"Run the test suite\"\n[[chore]]\ntitle = \"Review the docs\"\n";

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

#[test]
fn non_claim_post_and_rollback_are_no_ops() {
    let bl = FakeBl::new("[]");
    run("update", "post", "bl-chore", &wire("/x", &[], None), &bl).unwrap();
    run("claim", "pre", "bl-chore", &wire("/x", &[], None), &bl).unwrap();
    let rb = r#"{"binding":{"landing":"/x"},"rolling_back":"post"}"#;
    run("claim", "post", "bl-chore", rb, &bl).unwrap();
    assert!(bl.calls.borrow().is_empty());
}

#[test]
fn tag_skip_bails_when_the_claimed_task_carries_the_tag() {
    let bl = FakeBl::new("[]");
    run("claim", "post", "bl-chore", &wire("/x", &["bl-chore"], Some("bl-9")), &bl).unwrap();
    assert!(bl.calls.borrow().is_empty());
}

#[test]
fn empty_or_absent_config_mints_nothing() {
    let bl = FakeBl::new("[]");
    let tmp = TempDir::new().unwrap(); // no config file written
    run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(bl.calls.borrow().is_empty());
}

#[test]
fn a_payload_without_bl_id_is_an_error() {
    let bl = FakeBl::new("[]");
    let tmp = landing_with(TWO_CHORES);
    let err = run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], None), &bl);
    assert!(err.is_err());
}

#[test]
fn the_happy_path_mints_one_gate_per_chore() {
    let bl = FakeBl::new("[]"); // no children
    let tmp = landing_with(TWO_CHORES);
    run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    let creates = bl.creates();
    assert_eq!(creates.len(), 2);
    // Order + per-chore distinctness across the loop.
    assert_eq!(creates[0].last().unwrap(), "Run the test suite");
    assert_eq!(creates[1].last().unwrap(), "Review the docs");
    for c in &creates {
        assert!(c.contains(&"--parent".to_string()) && c.contains(&"bl-9".to_string()));
        assert!(c.contains(&"--blocks".to_string()) && c.contains(&"close".to_string()));
        assert!(c.contains(&"-t".to_string()) && c.contains(&"bl-chore".to_string()));
        // Authored as the CLAIMING actor (off the wire's distinctive "tester"),
        // not bl-chore's inherited identity — a regression to `--as bl-chore`
        // (the plugin name) would fail this.
        assert!(c.contains(&"--as".to_string()) && c.contains(&"tester".to_string()));
    }
    assert!(bl.listed()); // epic-skip queried (default on)
}

#[test]
fn body_and_priority_deserialize_and_thread_into_the_mint() {
    let bl = FakeBl::new("[]");
    let tmp = landing_with("[[chore]]\ntitle = \"Docs\"\nbody = \"check §6\"\npriority = 3\n");
    run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    let c = &bl.creates()[0];
    assert!(c.contains(&"--body".to_string()) && c.contains(&"check §6".to_string()));
    assert!(c.contains(&"-p".to_string()) && c.contains(&"3".to_string()));
}

#[test]
fn epic_skip_mints_when_the_only_child_belongs_to_another_parent() {
    let bl = FakeBl::new(r#"[{"parent":"bl-other"}]"#); // a child, but not of bl-9
    let tmp = landing_with(TWO_CHORES);
    run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(bl.listed() && bl.creates().len() == 2); // queried, foreign child != ours
}

#[test]
fn epic_skip_default_on_bails_when_the_task_has_children() {
    let bl = FakeBl::new(r#"[{"parent":"bl-9"}]"#); // bl-9 already has a child
    let tmp = landing_with(TWO_CHORES);
    run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(bl.listed() && bl.creates().is_empty());
}

#[test]
fn epic_skip_off_mints_without_the_child_query() {
    let bl = FakeBl::new(r#"[{"parent":"bl-9"}]"#); // would-be child, but knob off
    let tmp = landing_with(&format!("epic_skip = false\n{TWO_CHORES}"));
    run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(!bl.listed() && bl.creates().len() == 2);
}

#[test]
fn a_malformed_child_listing_is_an_error() {
    let bl = FakeBl::new("not json"); // epic-skip query returns garbage
    let tmp = landing_with(TWO_CHORES);
    assert!(run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).is_err());
}

#[test]
fn a_failed_create_aborts() {
    let mut bl = FakeBl::new("[]");
    bl.create_fails = true;
    let tmp = landing_with(TWO_CHORES);
    assert!(run("claim", "post", "bl-chore", &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).is_err());
}

#[test]
fn malformed_stdin_is_an_error() {
    let bl = FakeBl::new("[]");
    assert!(run("claim", "post", "bl-chore", "not json", &bl).is_err());
}
