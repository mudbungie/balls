//! `bl-chore` DISPATCH tests — every guard, the mint, and the §14 unwind, driven
//! through [`run`] against a fake [`Bl`] (no real `bl`) and temp config files.
//! The pure render/reader tests live in the sibling `chore_render_tests.rs`.

use super::*;
use std::cell::RefCell;
use tempfile::TempDir;

/// A fake [`Bl`] recording every call; `list` returns a scripted JSON, `create`
/// mints a sequential id on stdout (as the real `bl create` does — the id ALONE)
/// and succeeds while `creates_ok` allows.
struct FakeBl {
    list_json: String,
    /// How many `create`s succeed before the rest fail; `None` ⇒ all succeed.
    creates_ok: Option<usize>,
    calls: RefCell<Vec<Vec<String>>>,
}

impl FakeBl {
    fn new(list_json: &str) -> Self {
        Self { list_json: list_json.into(), creates_ok: None, calls: RefCell::new(Vec::new()) }
    }
    fn of_verb(&self, verb: &str) -> Vec<Vec<String>> {
        self.calls.borrow().iter().filter(|a| a.first().map(String::as_str) == Some(verb)).cloned().collect()
    }
    fn creates(&self) -> Vec<Vec<String>> {
        self.of_verb("create")
    }
    fn listed(&self) -> bool {
        !self.of_verb("list").is_empty()
    }
    /// The ids the recorded `bl close` calls named, in order.
    fn closed(&self) -> Vec<String> {
        self.of_verb("close").iter().map(|a| a[1].clone()).collect()
    }
}

impl Bl for FakeBl {
    fn run(&self, _cwd: &Path, argv: &[String]) -> io::Result<String> {
        self.calls.borrow_mut().push(argv.to_vec());
        let verb = argv.first().map(String::as_str);
        if verb == Some("list") {
            return Ok(self.list_json.clone());
        }
        if verb == Some("create") && self.creates_ok.is_some_and(|ok| self.creates().len() > ok) {
            return Err(io::Error::other("boom"));
        }
        // The minted id, trailing newline and all — what stdout really carries.
        Ok(format!("bl-c{}\n", self.creates().len()))
    }
}

/// A scratch territory for a run that mints nothing, so nothing is ever written.
fn nowhere() -> &'static Path {
    Path::new("/nonexistent-territory")
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

/// The same wire balls hands a `claim.post` plugin when the op ABORTS: identical
/// fields plus `rolling_back` (§7) — a post-phase unwind, so the sealed `bl-id`
/// is still on it.
fn rollback_wire(landing: &str, bl_id: Option<&str>) -> String {
    let mut v: serde_json::Value = serde_json::from_str(&wire(landing, &[], bl_id)).unwrap();
    v["rolling_back"] = serde_json::json!("post");
    v.to_string()
}

const TWO_CHORES: &str = "[[chore]]\ntitle = \"Run the test suite\"\n[[chore]]\ntitle = \"Review the docs\"\n";

#[test]
fn non_claim_post_and_an_unkeyed_rollback_are_no_ops() {
    let bl = FakeBl::new("[]");
    run("update", "post", "bl-chore", nowhere(), &wire("/x", &[], None), &bl).unwrap();
    run("claim", "pre", "bl-chore", nowhere(), &wire("/x", &[], None), &bl).unwrap();
    // A rollback wire carrying no sealed `bl-id` names no claim, so there is no
    // record keyed to it to unwind (and nothing was ever minted for one).
    let rb = r#"{"binding":{"landing":"/x"},"rolling_back":"post"}"#;
    run("claim", "post", "bl-chore", nowhere(), rb, &bl).unwrap();
    assert!(bl.calls.borrow().is_empty());
}

#[test]
fn tag_skip_bails_when_the_claimed_task_carries_the_tag() {
    let bl = FakeBl::new("[]");
    run("claim", "post", "bl-chore", nowhere(), &wire("/x", &["bl-chore"], Some("bl-9")), &bl).unwrap();
    assert!(bl.calls.borrow().is_empty());
}

#[test]
fn empty_or_absent_config_mints_nothing() {
    let bl = FakeBl::new("[]");
    let tmp = TempDir::new().unwrap(); // no config file written
    run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(bl.calls.borrow().is_empty());
}

#[test]
fn a_payload_without_bl_id_is_an_error() {
    let bl = FakeBl::new("[]");
    let tmp = landing_with(TWO_CHORES);
    let err = run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], None), &bl);
    assert!(err.is_err());
}

#[test]
fn the_happy_path_mints_one_gate_per_chore() {
    let bl = FakeBl::new("[]"); // no children
    let tmp = landing_with(TWO_CHORES);
    run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
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
    run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    let c = &bl.creates()[0];
    assert!(c.contains(&"--body".to_string()) && c.contains(&"check §6".to_string()));
    assert!(c.contains(&"-p".to_string()) && c.contains(&"3".to_string()));
}

#[test]
fn epic_skip_mints_when_the_only_child_belongs_to_another_parent() {
    let bl = FakeBl::new(r#"[{"parent":"bl-other"}]"#); // a child, but not of bl-9
    let tmp = landing_with(TWO_CHORES);
    run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(bl.listed() && bl.creates().len() == 2); // queried, foreign child != ours
}

#[test]
fn epic_skip_default_on_bails_when_the_task_has_children() {
    let bl = FakeBl::new(r#"[{"parent":"bl-9"}]"#); // bl-9 already has a child
    let tmp = landing_with(TWO_CHORES);
    run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(bl.listed() && bl.creates().is_empty());
}

#[test]
fn epic_skip_off_mints_without_the_child_query() {
    let bl = FakeBl::new(r#"[{"parent":"bl-9"}]"#); // would-be child, but knob off
    let tmp = landing_with(&format!("epic_skip = false\n{TWO_CHORES}"));
    run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).unwrap();
    assert!(!bl.listed() && bl.creates().len() == 2);
}

#[test]
fn a_malformed_child_listing_is_an_error() {
    let bl = FakeBl::new("not json"); // epic-skip query returns garbage
    let tmp = landing_with(TWO_CHORES);
    assert!(run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).is_err());
}

#[test]
fn a_failed_create_aborts_with_nothing_minted_to_take_down() {
    let mut bl = FakeBl::new("[]");
    bl.creates_ok = Some(0); // the very first create fails
    let tmp = landing_with(TWO_CHORES);
    assert!(run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).is_err());
    assert!(bl.closed().is_empty(), "nothing landed, so the inline cleanup has nothing to close");
}

#[test]
fn a_create_that_fails_midway_takes_the_landed_gates_back_down() {
    // §14: core never calls a FAILING plugin's own rollback — the plugin cleans
    // up INLINE before exiting non-zero. The first gate landed and the second
    // create failed, so the claim aborts AND the landed gate is closed: an
    // aborted claim leaves no orphan on either failure path (bl-ffbf).
    let mut bl = FakeBl::new("[]");
    bl.creates_ok = Some(1);
    let tmp = landing_with(TWO_CHORES);
    assert!(run("claim", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], Some("bl-9")), &bl).is_err());
    assert_eq!(bl.closed(), vec!["bl-c1"]);
}

#[test]
fn a_rolled_back_claim_closes_exactly_the_gates_it_minted() {
    // bl-ffbf/§14 appendix: each mint is a nested `bl create` with its own commit
    // point, sealed OUTSIDE the claiming op's atom — so an aborted claim's gates
    // are artifacts keyed to an op that never sealed, which nothing converges
    // onto. The rollback closes them, reading the ids from the record the
    // forward pass left in the plugin's own territory (§7 has no return channel
    // and no env crosses process boundaries). Consuming the record makes the
    // unwind idempotent, and a ball that minted nothing has none to read.
    let bl = FakeBl::new("[]");
    let tmp = landing_with(TWO_CHORES);
    let land = tmp.path().to_str().unwrap();
    run("claim", "post", "bl-chore", tmp.path(), &wire(land, &[], Some("bl-9")), &bl).unwrap();
    assert_eq!(bl.creates().len(), 2);

    run("claim", "post", "bl-chore", tmp.path(), &rollback_wire(land, Some("bl-9")), &bl).unwrap();
    assert_eq!(bl.closed(), vec!["bl-c1", "bl-c2"]);
    for closed in bl.of_verb("close") {
        // Authored as the CLAIMING actor, exactly like the mint it undoes.
        assert!(closed.contains(&"--as".to_string()) && closed.contains(&"tester".to_string()));
    }
    // Idempotent (§14): the record is consumed, so a second unwind closes nothing…
    run("claim", "post", "bl-chore", tmp.path(), &rollback_wire(land, Some("bl-9")), &bl).unwrap();
    // …and a ball this claim never minted for was never recorded at all.
    run("claim", "post", "bl-chore", tmp.path(), &rollback_wire(land, Some("bl-other")), &bl).unwrap();
    assert_eq!(bl.closed().len(), 2);
}

#[test]
fn a_close_retires_the_record_the_successful_claim_left() {
    // bl-f88b: §14 bounds scratch lifetime by the RESOURCE — "the plugin deletes
    // `<name>/<id>/` when the resource is gone (successful terminal op, or after
    // a rollback consumes it)". Only the rollback half was ever built, so every
    // claim that SUCCEEDED left a directory nothing would read, write, or delete
    // again. Closing the ball ends every claim of it, so that is where the
    // record dies — a delete, with no store query and no liveness predicate.
    let bl = FakeBl::new("[]");
    let tmp = landing_with(TWO_CHORES);
    let land = tmp.path().to_str().unwrap();
    run("claim", "post", "bl-chore", tmp.path(), &wire(land, &[], Some("bl-9")), &bl).unwrap();
    let record = tmp.path().join(crate::encoding::percent_encode("/proj")).join("bl-9");
    assert!(record.is_dir(), "the successful claim recorded its mints");

    run("close", "post", "bl-chore", tmp.path(), &wire(land, &[], Some("bl-9")), &bl).unwrap();
    assert!(!record.exists());
    // Idempotent, and scoped: a re-close, and a close of a ball bl-chore never
    // minted for, are both clean — the record is simply already absent.
    run("close", "post", "bl-chore", tmp.path(), &wire(land, &[], Some("bl-9")), &bl).unwrap();
    run("close", "post", "bl-chore", tmp.path(), &wire(land, &[], Some("bl-never")), &bl).unwrap();
    // A close only FORGETS: it never mints, and never closes what it forgets.
    assert!(bl.closed().is_empty() && bl.creates().len() == 2);
}

#[test]
fn a_close_wire_without_a_bl_id_names_no_record() {
    // On `claim.post` an absent `bl-id` is a contract violation (the mint has
    // nowhere to be keyed); here there is merely nothing to forget, so it is a
    // clean no-op — the same guarded bail the rollback takes.
    let bl = FakeBl::new("[]");
    let tmp = landing_with(TWO_CHORES);
    run("close", "post", "bl-chore", tmp.path(), &wire(tmp.path().to_str().unwrap(), &[], None), &bl).unwrap();
    assert!(bl.calls.borrow().is_empty());
}

#[test]
fn malformed_stdin_is_an_error() {
    let bl = FakeBl::new("[]");
    assert!(run("claim", "post", "bl-chore", nowhere(), "not json", &bl).is_err());
}
