//! Tests for the §16 `--legacy` read shim: the flag spelling, the git read off
//! a legacy `<ref>:<dir>`, and the pure projection (field map, implied tags,
//! dangling-parent nulling, ISO→epoch, the notes fold).

use tempfile::TempDir;

use super::*;
use crate::reads::test_support::legacy_store_at;
use crate::task::On;

/// A legacy repo in a fresh tempdir (see [`legacy_store_at`]).
fn legacy_repo(files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    legacy_store_at(tmp.path(), files);
    tmp
}

/// A minimal legacy task JSON with `status` and an optional parent.
fn legacy_json(id: &str, status: &str, parent: Option<&str>) -> String {
    let parent = parent.map_or("null".into(), |p| format!("\"{p}\""));
    format!(
        r#"{{"id":"{id}","title":"T {id}","status":"{status}","parent":{parent},
            "created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-02T03:04:06.123456Z"}}"#
    )
}

#[test]
fn flag_owns_the_legacy_spelling() {
    assert_eq!(flag("--legacy").as_deref(), Some(DEFAULT_SPEC));
    assert_eq!(flag("--legacy=v1:old/dir").as_deref(), Some("v1:old/dir"));
    assert_eq!(flag("--lega"), None);
    assert_eq!(flag("legacy"), None);
}

#[test]
fn balls_reads_the_ref_projects_live_tasks_and_skips_closed() {
    let repo = legacy_repo(&[
        ("bl-live.json", &legacy_json("bl-live", "open", None)),
        ("bl-dead.json", &legacy_json("bl-dead", "closed", None)),
        ("notes", "not a task file"), // non-.json content is ignored
    ]);
    let got = balls(repo.path(), DEFAULT_SPEC).unwrap();
    let ids: Vec<&str> = got.iter().map(|(id, _)| id.as_str()).collect();
    // Closed = file-absent in greenfield (§9): the dead task does not migrate.
    assert_eq!(ids, ["bl-live"]);
    let (_, task) = &got[0];
    assert_eq!(task.title, "T bl-live");
    // ISO→epoch, sub-second tail tolerated: 2026-01-02T03:04:05Z.
    assert_eq!(task.updated - task.created, 1);
}

#[test]
fn balls_refuses_a_missing_legacy_store_cleanly() {
    // bl-3ddb: a repo with no legacy ref died with git's raw `fatal: Not a
    // valid object name balls/tasks`; the shim owns the refusal and names
    // the spec instead.
    let tmp = TempDir::new().unwrap();
    crate::git::run(tmp.path(), &["init", "-q"], None).unwrap();
    let err = balls(tmp.path(), DEFAULT_SPEC).unwrap_err();
    assert_eq!(err.to_string(), format!("no legacy store at `{DEFAULT_SPEC}`"));
}

#[test]
fn balls_accepts_a_bare_ref_spec_and_names_a_bad_record() {
    let repo = legacy_repo(&[("bl-bad.json", "{not json")]);
    // A bare `<ref>` defaults the dir — same store, named parse failure.
    let err = balls(repo.path(), "balls/tasks").unwrap_err();
    assert!(err.to_string().contains("legacy .balls/tasks/bl-bad.json"), "{err}");
}

#[test]
fn project_maps_the_legacy_fields_into_the_greenfield_shape() {
    let t: Legacy = serde_json::from_str(
        r#"{"id":"bl-1","title":"One","status":"open","type":"epic",
            "created_at":"1970-01-01T00:00:10Z","updated_at":"1970-01-01T00:01:00Z",
            "claimed_by":"ann","parent":null,"priority":2,"tags":["epic","x"],
            "depends_on":["bl-2"],"description":"Top.","delivered_in":"abc"}"#,
    )
    .unwrap();
    let (id, task) = project(&[t], &[String::new()]).unwrap().pop().unwrap();
    assert_eq!(id, "bl-1");
    assert_eq!((task.created, task.updated), (10, 60));
    assert_eq!(task.claimant.as_deref(), Some("ann"));
    assert_eq!(task.priority, Some(2));
    // depends_on → claim-blockers; type:epic → tag, NOT duplicated when present.
    assert_eq!(task.blockers, [Blocker { id: "bl-2".into(), on: On::Claim }]);
    assert_eq!(task.tags, ["epic", "x"]);
    assert_eq!(task.body, "Top.\n");
    // Dropped legacy keys (`delivered_in`, …) have no core home — and no extras:
    // the projection is the §16 map alone, never a passthrough.
    assert!(task.extra.is_empty());
}

#[test]
fn project_implies_tags_nulls_dangling_parents_and_sorts() {
    let live: Vec<Legacy> = [
        legacy_json("bl-b", "deferred", Some("bl-a")),  // live parent: kept
        legacy_json("bl-a", "open", Some("bl-gone")),   // dangling: delinked
    ]
    .iter()
    .map(|j| serde_json::from_str(j).unwrap())
    .collect();
    let got = project(&live, &[String::new(), String::new()]).unwrap();
    // id-sorted output, whatever the store's file order.
    assert_eq!(got[0].0, "bl-a");
    assert_eq!(got[0].1.parent, None);
    assert_eq!(got[1].1.parent.as_deref(), Some("bl-a"));
    // status: deferred is a TAG in greenfield (§3) — implied once.
    assert_eq!(got[1].1.tags, ["deferred"]);
    assert!(got[0].1.tags.is_empty());
}

#[test]
fn epoch_refuses_a_malformed_timestamp() {
    for bad in ["2026-01-02", "2026-01-02 03:04:05", "soon", "2026-13-99T99:99:99Z"] {
        let err = epoch("bl-x", bad).unwrap_err();
        assert!(err.to_string().contains("bl-x"), "{bad} → {err}");
    }
}

#[test]
fn fold_notes_appends_a_section_and_skips_unparseable_lines() {
    let raw = concat!(
        r#"{"ts":"2025-01-01","author":"ann","text":"first"}"#, "\n",
        "garbage line\n",
        r#"{"ts":"2025-01-02","author":"bob","text":"second"}"#, "\n",
    );
    let body = fold_notes("Desc.", raw);
    assert_eq!(body, "Desc.\n\n## Notes (migrated)\n\n- 2025-01-01 ann: first\n- 2025-01-02 bob: second\n");
    // No notes: the description alone, newline-terminated; both empty: empty.
    assert_eq!(fold_notes("Desc.", ""), "Desc.\n");
    assert_eq!(fold_notes("", ""), "");
    // Notes with no description: the section stands alone.
    assert!(fold_notes("", raw).starts_with("## Notes (migrated)"));
}
