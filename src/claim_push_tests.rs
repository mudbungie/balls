//! Push-outcome classifier tests for the state-branch push.
//!
//! The retry-loop state machine is covered in `negotiation_tests.rs`
//! against a synthetic Protocol, and the `Participant`/`Protocol`
//! wiring lives in `claim_sync_tests.rs`. This file only exercises
//! the git-stderr classifier and the `is_unreachable` marker set.

use super::*;
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
    assert_eq!(classify_push_output(&out(true, "")), AttemptClass::Ok);
}

#[test]
fn classify_non_ff_rejected() {
    let s = "! [rejected] balls/tasks -> balls/tasks (non-fast-forward)";
    assert_eq!(classify_push_output(&out(false, s)), AttemptClass::Conflict);
}

#[test]
fn classify_fetch_first_rejected() {
    let s = "! [rejected] (fetch first)";
    assert_eq!(classify_push_output(&out(false, s)), AttemptClass::Conflict);
}

#[test]
fn classify_bracket_rejected_alone() {
    // Hits the third arm of the OR: stderr has "rejected" + "[rejected]"
    // but neither "non-fast-forward" nor "fetch first".
    let s = "warning: failed; rejected with [rejected] tag";
    assert_eq!(classify_push_output(&out(false, s)), AttemptClass::Conflict);
}

#[test]
fn classify_unreachable_dns() {
    let s = "fatal: Could not resolve hostname github.com";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        AttemptClass::Unreachable(_)
    ));
}

#[test]
fn classify_unreachable_repo_not_found() {
    let s = "ERROR: Repository not found.";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        AttemptClass::Unreachable(_)
    ));
}

#[test]
fn classify_other_falls_through() {
    let s = "fatal: weird unexpected error from git";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        AttemptClass::Other(_)
    ));
}

#[test]
fn is_unreachable_recognises_common_markers() {
    assert!(is_unreachable("connection refused"));
    assert!(is_unreachable("permission denied"));
    assert!(is_unreachable("network is unreachable"));
    assert!(!is_unreachable("totally fine"));
}
