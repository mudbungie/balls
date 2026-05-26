//! bl-fb4d conformance — command-level participant enforcement on
//! `bl update`, one test per failure policy.
//!
//! SPEC §9/§8.1/§11/§5.1, end-to-end through the real binary. The
//! negotiation primitive computes a required `reject` correctly
//! (pinned in `src/negotiation_reject_tests.rs`); this file proves
//! the *command* now consumes it:
//!
//! - required `reject` aborts `bl update` with the reason verbatim
//!   and rolls the state branch back (§9, conformance #19);
//! - best-effort `reject` ships and records `task.sync_status.<p>`;
//! - `--skip=NAME` ships past a required veto and logs `[--skip=NAME]`
//!   in the state-branch commit subject (§11, conformance #9);
//! - a `gating` policy degrades to `required` per bl-6969 (staging
//!   surface deferred): a gating `reject` aborts with the warning.
//!
//! `bl close` enforcement and the §5.1 side channel live in the
//! sibling `plugin_participant_enforce_close.rs`.

mod common;

use common::native_plugin::{create_auth, install_native_plugin, jira_policy, path_with};
use common::*;
use std::path::Path;

fn state_subject(repo: &Path) -> String {
    git_state(repo, &["log", "-1", "--format=%s", "balls/tasks"])
}

const REJECT: &str = r#"
    cat - >/dev/null
    printf '{"reject":{"reason":"CI is red on this branch"}}\n'
    exit 0
    "#;

#[test]
fn required_reject_aborts_update_and_rolls_back() {
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "enforced");
    jira_policy(repo.path(), "update", "required");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();

    assert!(!out.status.success(), "required veto must abort the command");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CI is red on this branch"),
        "reason must propagate verbatim: {stderr}"
    );
    // State branch rolled back: the appended note is gone with the
    // reverted update commit.
    let notes = read_task_notes(repo.path(), &id);
    assert!(
        !notes.iter().any(|n| n["text"] == "poke"),
        "the update commit must be rewound: {notes:?}"
    );
}

#[test]
fn best_effort_reject_ships_and_records_sync_status() {
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "best-effort");
    jira_policy(repo.path(), "update", "best-effort");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "best-effort veto must not abort: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task = read_task_json(repo.path(), &id);
    let recorded = task["sync_status"]["jira"].as_str().unwrap_or_default();
    assert!(
        recorded.contains("CI is red on this branch"),
        "best-effort skip records the verbatim reason: {task}"
    );
}

#[test]
fn skip_override_ships_past_required_and_is_logged() {
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "skipped");
    jira_policy(repo.path(), "update", "required");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--skip", "jira", "--note", "poke"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "--skip removes the required participant, so the event ships: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        state_subject(repo.path()).contains("[--skip=jira]"),
        "the §11 override must be logged in the state-branch commit: {}",
        state_subject(repo.path())
    );
    let notes = read_task_notes(repo.path(), &id);
    assert!(notes.iter().any(|n| n["text"] == "poke"), "update shipped");
}

#[test]
fn gating_reject_degrades_to_required_and_aborts() {
    // bl-6969: the staging surface is deferred, so PolicyKind::Gating
    // degrades to FailurePolicy::Required at the schema→runtime
    // boundary with a one-line warning. The observable contract is
    // the Required path — a reject aborts and the state branch rolls
    // back — plus the deferral warning on stderr so the operator
    // knows their config still parses but is being treated leniently.
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "gated");
    jira_policy(repo.path(), "update", "gating");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "gating degrades to required (bl-6969); a reject must abort"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("staging is deferred (see bl-6969)"),
        "the degrade warning must fire so operators see the config is read leniently: {stderr}"
    );
    assert!(
        stderr.contains("CI is red on this branch"),
        "the reject reason still propagates verbatim: {stderr}"
    );
    let notes = read_task_notes(repo.path(), &id);
    assert!(
        !notes.iter().any(|n| n["text"] == "poke"),
        "the update commit must be rewound under the required degrade: {notes:?}"
    );
}
