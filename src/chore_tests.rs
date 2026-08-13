//! `bl-chore` tests — every guard and the mint itself, driven through [`run`]
//! against a REAL change worktree (a temp dir holding `tasks/`), which is what
//! the plugin now writes. There is no `bl` to fake and no rollback to drive:
//! since bl-1da3 the mint is a file write inside the claim's own atom.

use super::*;
use tempfile::TempDir;

/// A change worktree holding one parent ball, staged as `claim` leaves it —
/// `updated` already bumped to the op instant, which is the clock the children
/// inherit.
fn worktree(parent_id: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let parent = Task {
        title: "The claimed ball".into(),
        created: 100,
        updated: 4242, // the op instant `base.stage` stamped
        claimant: Some("tester".into()),
        root_commit: Some("deadbeef".into()),
        ..Task::default()
    };
    write_task(tmp.path(), parent_id, &parent).unwrap();
    tmp
}

/// A temp landing whose `config/plugins/bl-chore/chores.toml` holds `toml`.
fn landing_with(toml: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("config/plugins/bl-chore");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("chores.toml"), toml).unwrap();
    tmp
}

/// A `claim.pre` wire: the landing, the op-start tags, and the ball the op is
/// about (§7 `command.id`, present on a `pre` payload).
fn wire(landing: &str, tags: &[&str], id: Option<&str>) -> String {
    serde_json::json!({
        "binding": { "landing": landing },
        "command": { "op": "claim", "id": id },
        "current_state": { "tags": tags },
    })
    .to_string()
}

const TWO_CHORES: &str = "[[chore]]\ntitle = \"Run the test suite\"\n[[chore]]\ntitle = \"Review the docs\"\n";

/// Every `tasks/*.md` in `dir` except `except`, as (id, task) pairs.
fn children(dir: &Path, except: &str) -> Vec<(String, Task)> {
    let mut out: Vec<(String, Task)> = task_ids(dir)
        .unwrap()
        .into_iter()
        .filter(|id| id != except)
        .map(|id| {
            let t = read_task(dir, &id).unwrap();
            (id, t)
        })
        .collect();
    out.sort_by(|a, b| a.1.title.cmp(&b.1.title));
    out
}

#[test]
fn only_the_claim_pre_forward_pass_does_anything() {
    let land = landing_with(TWO_CHORES);
    let l = land.path().to_string_lossy().into_owned();
    let w = worktree("bl-9");
    let mint_nothing = |op: &str, phase: &str, payload: &str| {
        run(op, phase, "bl-chore", w.path(), payload).unwrap();
        assert_eq!(children(w.path(), "bl-9"), vec![], "{op}.{phase} minted");
    };
    mint_nothing("update", "pre", &wire(&l, &[], Some("bl-9")));
    mint_nothing("claim", "post", &wire(&l, &[], Some("bl-9")));
    // A rollback is a no-op BY CONSTRUCTION: whatever a forward pass wrote is in
    // the change worktree core discards, so there is nothing keyed to unwind.
    let mut rb: serde_json::Value = serde_json::from_str(&wire(&l, &[], Some("bl-9"))).unwrap();
    rb["rolling_back"] = serde_json::json!("pre");
    mint_nothing("claim", "pre", &rb.to_string());
}

#[test]
fn tag_skip_bails_when_the_claimed_task_carries_the_tag() {
    let land = landing_with(TWO_CHORES);
    let w = worktree("bl-9");
    let payload = wire(&land.path().to_string_lossy(), &["bl-chore"], Some("bl-9"));
    run("claim", "pre", "bl-chore", w.path(), &payload).unwrap();
    assert_eq!(children(w.path(), "bl-9"), vec![], "a chore must not mint a chore-of-a-chore");
}

#[test]
fn an_absent_or_choreless_config_mints_nothing() {
    let w = worktree("bl-9");
    for landing in ["/nonexistent-landing".to_string(), landing_with("").path().to_string_lossy().into_owned()] {
        run("claim", "pre", "bl-chore", w.path(), &wire(&landing, &[], Some("bl-9"))).unwrap();
        assert_eq!(children(w.path(), "bl-9"), vec![]);
    }
}

#[test]
fn the_mint_writes_each_chore_as_a_close_gate_child_of_the_claimed_ball() {
    let land = landing_with(
        "[[chore]]\ntitle = \"Run the test suite\"\nbody = \"cargo test\"\npriority = 3\n\
         [[chore]]\ntitle = \"Review the docs\"\n",
    );
    let w = worktree("bl-9");
    run("claim", "pre", "bl-chore", w.path(), &wire(&land.path().to_string_lossy(), &[], Some("bl-9"))).unwrap();

    let kids = children(w.path(), "bl-9");
    let titles: Vec<&str> = kids.iter().map(|(_, t)| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Review the docs", "Run the test suite"]);
    for (id, t) in &kids {
        assert!(crate::id::is_valid(id), "minted a malformed id: {id}");
        assert_eq!(t.parent.as_deref(), Some("bl-9"));
        assert_eq!(t.tags, vec!["bl-chore".to_string()], "the recursion-break tag is always injected");
        // The op instant and the repo identity are INHERITED, never re-derived.
        assert_eq!((t.created, t.updated), (4242, 4242));
        assert_eq!(t.root_commit.as_deref(), Some("deadbeef"));
    }
    let suite = &kids[1].1;
    assert_eq!(suite.priority, Some(3));
    assert_eq!(suite.body, "cargo test");

    // The gate edge lands on the PARENT — one `{id, on: close}` blocker per chore.
    let parent = read_task(w.path(), "bl-9").unwrap();
    let gated: Vec<&str> = parent.blockers.iter().map(|b| b.id.as_str()).collect();
    let mut minted: Vec<&str> = kids.iter().map(|(id, _)| id.as_str()).collect();
    minted.sort_unstable();
    let mut gated_sorted = gated.clone();
    gated_sorted.sort_unstable();
    assert_eq!(gated_sorted, minted);
    assert!(parent.blockers.iter().all(|b| b.on == On::Close));
}

#[test]
fn epic_skip_bails_on_an_existing_child_and_the_knob_turns_it_off() {
    let land = landing_with(TWO_CHORES);
    let l = land.path().to_string_lossy().into_owned();
    let w = worktree("bl-9");
    let kid = Task { title: "A real subtask".into(), parent: Some("bl-9".into()), ..Task::default() };
    write_task(w.path(), "bl-kid", &kid).unwrap();

    run("claim", "pre", "bl-chore", w.path(), &wire(&l, &[], Some("bl-9"))).unwrap();
    assert_eq!(children(w.path(), "bl-9").len(), 1, "epic-skip: only the pre-existing child");

    // The same worktree with the knob off mints anyway.
    let off = landing_with(&format!("epic_skip = false\n{TWO_CHORES}"));
    let payload = wire(&off.path().to_string_lossy(), &[], Some("bl-9"));
    run("claim", "pre", "bl-chore", w.path(), &payload).unwrap();
    assert_eq!(children(w.path(), "bl-9").len(), 3);
}

#[test]
fn a_ball_with_an_unrelated_child_is_not_epic_skipped() {
    let land = landing_with(TWO_CHORES);
    let w = worktree("bl-9");
    let other = Task { title: "Child of someone else".into(), parent: Some("bl-other".into()), ..Task::default() };
    write_task(w.path(), "bl-oth", &other).unwrap();
    run("claim", "pre", "bl-chore", w.path(), &wire(&land.path().to_string_lossy(), &[], Some("bl-9"))).unwrap();
    assert_eq!(children(w.path(), "bl-9").len(), 3); // the stranger + two chores
}

#[test]
fn a_wire_that_names_no_ball_is_a_contract_violation() {
    let land = landing_with(TWO_CHORES);
    let w = worktree("bl-9");
    let err = run("claim", "pre", "bl-chore", w.path(), &wire(&land.path().to_string_lossy(), &[], None))
        .unwrap_err()
        .to_string();
    assert!(err.contains("names no ball"), "{err}");
}

#[test]
fn a_malformed_payload_or_config_aborts_the_claim() {
    let w = worktree("bl-9");
    let bad_wire = run("claim", "pre", "bl-chore", w.path(), "not json").unwrap_err().to_string();
    assert!(bad_wire.contains("expected"), "{bad_wire}");

    let land = landing_with("[[chore]]\nnot_a_title = 1\n");
    let bad_cfg = run("claim", "pre", "bl-chore", w.path(), &wire(&land.path().to_string_lossy(), &[], Some("bl-9")))
        .unwrap_err()
        .to_string();
    assert!(bad_cfg.contains("title"), "{bad_cfg}");

    // A config path that is not a readable file at all (a DIRECTORY where the
    // toml should be) is neither absent nor parseable — the read error rides out.
    let land = TempDir::new().unwrap();
    fs::create_dir_all(land.path().join("config/plugins/bl-chore/chores.toml")).unwrap();
    let payload = wire(&land.path().to_string_lossy(), &[], Some("bl-9"));
    assert!(run("claim", "pre", "bl-chore", w.path(), &payload).is_err());
}

#[test]
fn an_unreadable_sibling_or_missing_parent_aborts_rather_than_minting_blind() {
    let land = landing_with(TWO_CHORES);
    let l = land.path().to_string_lossy().into_owned();

    // epic-skip reads every ball in the worktree; one that does not parse is a
    // store it cannot answer over, so the claim aborts instead of guessing.
    let w = worktree("bl-9");
    fs::write(w.path().join("tasks").join("bl-bad.md"), "not a ball").unwrap();
    assert!(run("claim", "pre", "bl-chore", w.path(), &wire(&l, &[], Some("bl-9"))).is_err());

    // The named ball is not in the worktree: nothing to inherit a clock or a
    // root_commit from, and nothing to hang the gate on.
    let empty = TempDir::new().unwrap();
    let off = landing_with(&format!("epic_skip = false\n{TWO_CHORES}"));
    let payload = wire(&off.path().to_string_lossy(), &[], Some("bl-9"));
    assert!(run("claim", "pre", "bl-chore", empty.path(), &payload).is_err());
}
