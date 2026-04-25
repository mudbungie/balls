use super::*;
use std::cell::RefCell;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

fn out(success: bool, stderr: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(if success { 0 } else { 1 << 8 }),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn classify_success() {
    assert_eq!(classify_push_output(&out(true, "")), PushClass::Ok);
}

#[test]
fn classify_non_ff_rejected() {
    let s = "! [rejected] balls/tasks -> balls/tasks (non-fast-forward)";
    assert_eq!(classify_push_output(&out(false, s)), PushClass::Rejected);
}

#[test]
fn classify_fetch_first_rejected() {
    let s = "! [rejected] (fetch first)";
    assert_eq!(classify_push_output(&out(false, s)), PushClass::Rejected);
}

#[test]
fn classify_bracket_rejected_alone() {
    // Hit the third arm of the OR: stderr has "rejected" + "[rejected]"
    // but neither "non-fast-forward" nor "fetch first".
    let s = "warning: failed; rejected with [rejected] tag";
    assert_eq!(classify_push_output(&out(false, s)), PushClass::Rejected);
}

#[test]
fn classify_unreachable_dns() {
    let s = "fatal: Could not resolve hostname github.com";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        PushClass::Unreachable(_)
    ));
}

#[test]
fn classify_unreachable_repo_not_found() {
    let s = "ERROR: Repository not found.";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        PushClass::Unreachable(_)
    ));
}

#[test]
fn classify_other_falls_through() {
    let s = "fatal: weird unexpected error from git";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        PushClass::Other(_)
    ));
}

#[test]
fn is_unreachable_recognises_common_markers() {
    assert!(is_unreachable("connection refused"));
    assert!(is_unreachable("permission denied"));
    assert!(is_unreachable("network is unreachable"));
    assert!(!is_unreachable("totally fine"));
}

// ---- run_push_loop state-machine coverage ----

#[allow(clippy::unnecessary_wraps)]
fn ok_merge() -> Result<()> { Ok(()) }

#[test]
fn loop_first_push_ok_returns_pushed() {
    let r = run_push_loop(
        "alice",
        3,
        || Ok(PushClass::Ok),
        ok_merge,
        || Ok(Some("alice".into())),
    )
    .unwrap();
    assert_eq!(r, SyncedClaimResult::Pushed);
}

#[test]
fn loop_rejected_then_ok_retries() {
    let calls = RefCell::new(0usize);
    let push = || {
        let mut c = calls.borrow_mut();
        *c += 1;
        if *c == 1 { Ok(PushClass::Rejected) } else { Ok(PushClass::Ok) }
    };
    let r = run_push_loop(
        "alice",
        5,
        push,
        ok_merge,
        || Ok(Some("alice".into())),
    )
    .unwrap();
    assert_eq!(r, SyncedClaimResult::Pushed);
    assert_eq!(*calls.borrow(), 2);
}

#[test]
fn loop_lost_when_merge_replaces_claimer() {
    let calls = RefCell::new(0usize);
    let push = || {
        *calls.borrow_mut() += 1;
        Ok(PushClass::Rejected)
    };
    let r = run_push_loop(
        "alice",
        5,
        push,
        ok_merge,
        || Ok(Some("bob".into())),
    )
    .unwrap();
    assert_eq!(r, SyncedClaimResult::Lost { winner: "bob".into() });
    // One rejected push, then a best-effort post-merge push.
    assert_eq!(*calls.borrow(), 2);
}

#[test]
fn loop_lost_with_no_claimer_uses_unknown() {
    let r = run_push_loop(
        "alice",
        5,
        || Ok(PushClass::Rejected),
        ok_merge,
        || Ok(None),
    )
    .unwrap();
    assert_eq!(r, SyncedClaimResult::Lost { winner: "(unknown)".into() });
}

#[test]
fn loop_unreachable_mid_flight_errors() {
    let err = run_push_loop(
        "alice",
        3,
        || Ok(PushClass::Unreachable("net down".into())),
        ok_merge,
        || Ok(Some("alice".into())),
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("net down"), "msg: {msg}");
}

#[test]
fn loop_other_push_failure_errors() {
    let err = run_push_loop(
        "alice",
        3,
        || Ok(PushClass::Other("weird".into())),
        ok_merge,
        || Ok(Some("alice".into())),
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("weird"), "msg: {msg}");
}

#[test]
fn loop_max_retries_exhausted() {
    let calls = RefCell::new(0usize);
    let push = || {
        *calls.borrow_mut() += 1;
        Ok(PushClass::Rejected)
    };
    let err = run_push_loop(
        "alice",
        3,
        push,
        ok_merge,
        || Ok(Some("alice".into())),
    )
    .unwrap_err();
    assert!(format!("{err}").contains("gave up"));
    assert_eq!(*calls.borrow(), 3);
}

#[test]
fn loop_propagates_push_error() {
    let err = run_push_loop(
        "alice",
        3,
        || Err(BallError::Other("spawn failed".into())),
        ok_merge,
        || Ok(Some("alice".into())),
    )
    .unwrap_err();
    assert!(format!("{err}").contains("spawn"));
}

#[test]
fn loop_propagates_merge_error() {
    let err = run_push_loop(
        "alice",
        3,
        || Ok(PushClass::Rejected),
        || Err(BallError::Conflict("unresolvable".into())),
        || Ok(Some("alice".into())),
    )
    .unwrap_err();
    assert!(format!("{err}").contains("unresolvable"));
}
