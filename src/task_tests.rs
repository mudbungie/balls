use super::*;
use tempfile::TempDir;

#[test]
fn id_generation_is_deterministic() {
    let ts = Utc::now();
    let a = Task::generate_id("Hello", ts, 4);
    let b = Task::generate_id("Hello", ts, 4);
    assert_eq!(a, b);
    assert!(a.starts_with("bl-"));
    assert_eq!(a.len(), 7);
}

#[test]
fn id_length_respected() {
    let ts = Utc::now();
    let a = Task::generate_id("Hello", ts, 6);
    assert_eq!(a.len(), 9);
}

#[test]
fn roundtrip_serialization() {
    let opts = NewTaskOpts {
        title: "test".to_string(),
        ..Default::default()
    };
    let id = Task::generate_id("test", Utc::now(), 4);
    let t = Task::new(opts, id.clone());
    let s = serde_json::to_string(&t).unwrap();
    let t2: Task = serde_json::from_str(&s).unwrap();
    assert_eq!(t.id, t2.id);
    assert_eq!(t.title, t2.title);
}

#[test]
fn task_type_parse_all() {
    assert_eq!(TaskType::parse("epic").unwrap(), TaskType::Epic);
    assert_eq!(TaskType::parse("task").unwrap(), TaskType::Task);
    assert_eq!(TaskType::parse("bug").unwrap(), TaskType::Bug);
    assert!(TaskType::parse("nope").is_err());
}

#[test]
fn status_parse_all() {
    assert_eq!(Status::parse("open").unwrap(), Status::Open);
    assert_eq!(Status::parse("in_progress").unwrap(), Status::InProgress);
    assert_eq!(Status::parse("review").unwrap(), Status::Review);
    assert_eq!(Status::parse("blocked").unwrap(), Status::Blocked);
    assert_eq!(Status::parse("closed").unwrap(), Status::Closed);
    assert_eq!(Status::parse("deferred").unwrap(), Status::Deferred);
    assert!(Status::parse("bogus").is_err());
}

#[test]
fn status_precedence_and_str() {
    assert!(Status::Closed.precedence() > Status::Review.precedence());
    assert!(Status::Review.precedence() > Status::InProgress.precedence());
    assert!(Status::InProgress.precedence() > Status::Blocked.precedence());
    assert!(Status::Blocked.precedence() > Status::Open.precedence());
    assert!(Status::Open.precedence() > Status::Deferred.precedence());
    assert_eq!(Status::Open.as_str(), "open");
    assert_eq!(Status::InProgress.as_str(), "in_progress");
    assert_eq!(Status::Review.as_str(), "review");
    assert_eq!(Status::Blocked.as_str(), "blocked");
    assert_eq!(Status::Closed.as_str(), "closed");
    assert_eq!(Status::Deferred.as_str(), "deferred");
}

#[test]
fn status_unknown_is_lowest_precedence() {
    // Unknown must never win a conflict-resolution race against a real
    // status — `Status::Deferred` is the lowest known value and Unknown
    // must sit strictly below it.
    let u = Status::Unknown("triaged".to_string());
    assert!(u.precedence() < Status::Deferred.precedence());
    assert_eq!(u.as_str(), "triaged");
    assert_eq!(format!("{u}"), "triaged");
}

#[test]
fn status_deserialize_unknown_preserves_string() {
    // Forward-compat: an older binary reading a JSON file written by a
    // future bl version must not hard-error on the whole task file. The
    // unknown variant is preserved verbatim and re-serializes unchanged.
    let back: Status = serde_json::from_str("\"from_the_future\"").unwrap();
    assert_eq!(back, Status::Unknown("from_the_future".to_string()));
    let s = serde_json::to_string(&back).unwrap();
    assert_eq!(s, "\"from_the_future\"");
}

#[test]
fn status_parse_rejects_unknown_cli_input() {
    // parse() is the CLI entry point — it must refuse unknown strings
    // so `bl update status=...` can't silently create Unknown variants.
    // Only deserialization produces Unknown.
    assert!(Status::parse("from_the_future").is_err());
    assert!(Status::parse("").is_err());
}

#[test]
fn status_known_round_trip_preserves_variant() {
    for s in [
        Status::Open,
        Status::InProgress,
        Status::Review,
        Status::Blocked,
        Status::Closed,
        Status::Deferred,
    ] {
        let j = serde_json::to_string(&s).unwrap();
        let back: Status = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }
}

#[test]
fn save_and_load_task_file() {
    let dir = TempDir::new().unwrap();
    let t = Task::new(
        NewTaskOpts {
            title: "saveme".into(),
            ..Default::default()
        },
        "bl-ae5f".into(),
    );
    let path = dir.path().join("bl-ae5f.json");
    t.save(&path).unwrap();
    let back = Task::load(&path).unwrap();
    assert_eq!(back.id, "bl-ae5f");
}

#[test]
fn load_malformed_reports_invalid_task() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, "{ not json").unwrap();
    let err = Task::load(&path).unwrap_err();
    assert!(matches!(err, BallError::InvalidTask(_)));
}

#[test]
fn touch_updates_timestamp() {
    let mut t = Task::new(
        NewTaskOpts {
            title: "t".into(),
            ..Default::default()
        },
        "bl-t".into(),
    );
    let before = t.updated_at;
    std::thread::sleep(std::time::Duration::from_millis(2));
    t.touch();
    assert!(t.updated_at > before);
}

#[test]
fn append_note_adds_entry() {
    let mut t = Task::new(
        NewTaskOpts {
            title: "t".into(),
            ..Default::default()
        },
        "bl-t".into(),
    );
    t.append_note("alice", "hi");
    assert_eq!(t.notes.len(), 1);
    assert_eq!(t.notes[0].author, "alice");
    assert_eq!(t.notes[0].text, "hi");
}

#[test]
fn default_new_task_opts() {
    let o = NewTaskOpts::default();
    assert_eq!(o.priority, 3);
    assert!(matches!(o.task_type, TaskType::Task));
}

#[test]
fn display_impls() {
    assert_eq!(format!("{}", TaskType::Epic), "epic");
    assert_eq!(format!("{}", TaskType::Task), "task");
    assert_eq!(format!("{}", TaskType::Bug), "bug");
    assert_eq!(format!("{}", Status::Open), "open");
    assert_eq!(format!("{}", Status::InProgress), "in_progress");
}

#[test]
fn validate_id_accepts_valid() {
    assert!(validate_id("bl-a1b2").is_ok());
    assert!(validate_id("bl-deadbeef").is_ok());
    assert!(validate_id("bl-0000").is_ok());
    assert!(validate_id("bl-abcdef0123456789").is_ok());
}

#[test]
fn validate_id_rejects_path_traversal() {
    assert!(validate_id("../../../etc/passwd").is_err());
    assert!(validate_id("bl-../../etc").is_err());
    assert!(validate_id("..").is_err());
}

#[test]
fn validate_id_rejects_malformed() {
    assert!(validate_id("").is_err());
    assert!(validate_id("bl-").is_err());
    assert!(validate_id("not-a-task").is_err());
    assert!(validate_id("bl-UPPER").is_err());
    assert!(validate_id("bl-a1b2/subdir").is_err());
    assert!(validate_id("BL-a1b2").is_err());
}
