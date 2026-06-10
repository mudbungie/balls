//! Tests for `bl import` (§16): the flag parse, the refuse-the-stream collision
//! check, the verbatim write-through, and the `--legacy` cutover button (the
//! in-process `bl list --legacy --json | bl import` pipe + the epic edge pass).

use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::reads::test_support::{edge as edge_in, legacy_store_at};
use crate::task::On;
use crate::taskfile::read_task;

fn strs(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

/// An edge in `tmp` with its checkout founded (stealth — no plugin binaries, so
/// the seed prunes every default hook and ops run plugin-free).
fn primed(tmp: &TempDir) -> Edge {
    let edge = edge_in(tmp.path(), 0);
    assert_eq!(crate::run(&edge, &strs(&["prime", "--as", "me"])), 0);
    edge
}

/// The founded checkout's store dir (where `tasks/<id>.md` land).
fn store(edge: &Edge) -> PathBuf {
    edge.xdg.clone_dir(&edge.invocation_path).store()
}

/// A minimal fully-identified bedrock record for `id`.
fn record(id: &str) -> String {
    format!(r#"{{"id":"{id}","title":"T {id}","created":5,"updated":9,"body":"kept\n"}}"#)
}

#[test]
fn parse_speaks_the_mutating_flag_vocabulary() {
    let f = parse(&strs(&["--as", "ann", "-m", "note", "--remote", "r", "--center", "c"]), "default").unwrap();
    assert_eq!(f.actor, "ann");
    assert_eq!(f.message.as_deref(), Some("note"));
    // `--remote` always assigns; `--center` fills only an empty slot (§12).
    assert_eq!(f.remote.as_deref(), Some("r"));
    assert!(f.legacy.is_none());
    let f = parse(&strs(&["--center", "c", "--legacy=v1:d"]), "me").unwrap();
    assert_eq!(f.remote.as_deref(), Some("c"));
    assert_eq!(f.legacy.as_deref(), Some("v1:d"));
    assert_eq!(parse(&strs(&["--legacy"]), "me").unwrap().legacy.as_deref(), Some(crate::reads::legacy::DEFAULT_SPEC));
    assert_eq!(parse(&[], "default").unwrap().actor, "default");
}

#[test]
fn parse_refuses_a_positional_and_a_valueless_flag() {
    // Records ride stdin — a positional has nowhere to go.
    let err = parse(&strs(&["task.json"]), "me").unwrap_err();
    assert!(err.to_string().contains("records ride stdin"), "{err}");
    for flag in ["--as", "-m", "--remote", "--center"] {
        let err = parse(&strs(&[flag]), "me").unwrap_err();
        assert!(err.to_string().contains("needs a value"), "{flag}: {err}");
    }
}

#[test]
fn stage_refuses_the_whole_stream_on_any_collision() {
    let tmp = TempDir::new().unwrap();
    let held = stream::records(&record("bl-held")).unwrap();
    crate::taskfile::write_task(tmp.path(), "bl-held", &held[0].1).unwrap();
    // One stored collision + one in-stream duplicate, both named; the fresh
    // record between them is NOT written — all-or-nothing.
    let text = format!("{}{}{}{}", record("bl-held"), record("bl-new"), record("bl-dup"), record("bl-dup"));
    let base = Ingest { balls: stream::records(&text).unwrap(), actor: "me".into(), message: None };
    let err = base.stage(tmp.path()).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(err.to_string().contains("bl-held, bl-dup"), "{err}");
    assert!(!crate::taskfile::exists(tmp.path(), "bl-new"));
}

#[test]
fn finalize_counts_the_subject_and_narrated_tracks_the_note() {
    let one = Ingest { balls: stream::records(&record("bl-1")).unwrap(), actor: "me".into(), message: None };
    assert!(one.finalize(&PathBuf::new()).unwrap().starts_with("import 1 ball\n"));
    assert!(!one.narrated());
    let two = Ingest {
        balls: stream::records(&format!("{}{}", record("bl-1"), record("bl-2"))).unwrap(),
        actor: "me".into(),
        message: Some("why".into()),
    };
    let msg = two.finalize(&PathBuf::new()).unwrap();
    assert!(msg.starts_with("import 2 balls\n"), "{msg}");
    assert!(msg.contains("why"));
    // A bulk op names no single ball — the §5 id-less form, no `bl-id` trailer.
    assert!(!msg.contains("bl-id:"));
    assert!(two.narrated());
}

#[test]
fn run_writes_the_stream_through_the_real_store_verbatim() {
    let tmp = TempDir::new().unwrap();
    let edge = primed(&tmp);
    let text = format!("[{},{}]", record("bl-i1"), record("bl-i2"));
    run(&edge, &mut text.as_bytes(), &strs(&["--as", "me"])).unwrap();
    let task = read_task(&store(&edge), "bl-i1").unwrap();
    // Verbatim reproduction: foreign id, historical timestamps, body — exactly
    // what `create` refuses (it mints + stamps), which is why import exists.
    assert_eq!((task.created, task.updated), (5, 9));
    assert_eq!(task.body, "kept\n");
    assert!(crate::taskfile::exists(&store(&edge), "bl-i2"));
    // The seal landed as ONE store commit; a re-import of a held id refuses.
    let err = run(&edge, &mut record("bl-i1").as_bytes(), &strs(&["--as", "me"])).unwrap_err();
    assert!(err.to_string().contains("bl-i1"), "{err}");
}

#[test]
fn run_with_narration_on_an_empty_stream_fails_loud() {
    let tmp = TempDir::new().unwrap();
    let edge = primed(&tmp);
    // bl-cf93: a converged (zero-record) seal would silently drop the `-m`
    // note — abort instead. Without narration the empty stream is a no-op.
    assert!(run(&edge, &mut "".as_bytes(), &strs(&["-m", "note"])).is_err());
    run(&edge, &mut "".as_bytes(), &strs(&[])).unwrap();
}

#[test]
fn the_legacy_button_imports_live_balls_and_wires_the_epic_edges() {
    let tmp = TempDir::new().unwrap();
    let iso = r#""created_at":"2026-01-01T00:00:07Z","updated_at":"2026-01-01T00:00:08Z""#;
    legacy_store_at(
        &tmp.path().join("proj"),
        &[
            ("bl-ep.json", &format!(r#"{{"id":"bl-ep","title":"Epic","status":"open","type":"epic",{iso}}}"#)),
            ("bl-c1.json", &format!(r#"{{"id":"bl-c1","title":"Kid","status":"open","parent":"bl-ep",{iso}}}"#)),
            ("bl-c2.json", &format!(r#"{{"id":"bl-c2","title":"Done","status":"closed","parent":"bl-ep",{iso}}}"#)),
            ("bl-c3.json", &format!(r#"{{"id":"bl-c3","title":"Waif","status":"open","parent":"bl-gone",{iso}}}"#)),
        ],
    );
    let edge = primed(&tmp);
    // Pure orchestration: list --legacy --json | import, in-process, then the
    // reciprocal edge pass — stdin is never read on this path. The per-op
    // `--remote` override threads through to the edge ops too (no tracker is
    // bound here, so nothing remote-talks — §12: remote-talk is the tracker's).
    run(&edge, &mut std::io::empty(), &strs(&["--legacy", "--as", "me", "--remote", "unused"])).unwrap();
    let store = store(&edge);
    // Live tasks migrated, the closed one skipped (file-absent = resolved, §9).
    assert!(!crate::taskfile::exists(&store, "bl-c2"));
    let kid = read_task(&store, "bl-c1").unwrap();
    assert_eq!(kid.parent.as_deref(), Some("bl-ep"));
    assert_eq!((kid.created, kid.updated), (1_767_225_607, 1_767_225_608));
    // The dangling parent was delinked by the projection.
    assert_eq!(read_task(&store, "bl-c3").unwrap().parent, None);
    // The epic edge rode the ORDINARY update --needs machinery: the live child
    // claim-blocks its parent, so the epic is not spuriously ready (§16).
    let epic = read_task(&store, "bl-ep").unwrap();
    assert_eq!(epic.blockers, [crate::task::Blocker { id: "bl-c1".into(), on: On::Claim }]);
    assert!(epic.tags.contains(&"epic".to_string()));
}

#[test]
fn the_legacy_button_completes_through_the_dispatch() {
    // bl-0a80: the dispatch must not hold the host stdin lock across the verb —
    // the legacy edge pass re-enters stdin through `mutate::run`'s editor seam
    // (`Editor::live` locks stdin), and the std stdin mutex is non-reentrant,
    // so a held lock self-deadlocks after the node seal. Run the REAL dispatch
    // in a thread and demand it finishes: under the bug this starves forever.
    let tmp = TempDir::new().unwrap();
    let iso = r#""created_at":"2026-01-01T00:00:07Z","updated_at":"2026-01-01T00:00:08Z""#;
    legacy_store_at(
        &tmp.path().join("proj"),
        &[
            ("bl-ep.json", &format!(r#"{{"id":"bl-ep","title":"Epic","status":"open","type":"epic",{iso}}}"#)),
            ("bl-c1.json", &format!(r#"{{"id":"bl-c1","title":"Kid","status":"open","parent":"bl-ep",{iso}}}"#)),
        ],
    );
    let edge = primed(&tmp);
    let (tx, rx) = std::sync::mpsc::channel();
    let dispatched = edge.clone();
    std::thread::spawn(move || {
        let _ = tx.send(crate::run(&dispatched, &strs(&["import", "--legacy", "--as", "me"])));
    });
    let code = rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("import --legacy deadlocked: the stdin lock was held across the epic edge pass");
    assert_eq!(code, 0);
    // The whole button ran: nodes sealed AND the epic edge pass landed.
    let epic = read_task(&store(&edge), "bl-ep").unwrap();
    assert_eq!(epic.blockers, [crate::task::Blocker { id: "bl-c1".into(), on: On::Claim }]);
}

#[test]
fn the_dispatch_wires_import_and_a_parse_error_exits_one() {
    // Through `crate::run` (the §8 dispatch): the verb is reachable, and a bad
    // argv fails BEFORE the stdin read (so the test never touches the host's).
    let tmp = TempDir::new().unwrap();
    let edge = edge_in(tmp.path(), 0);
    assert_eq!(crate::run(&edge, &strs(&["import", "bogus"])), 1);
}
