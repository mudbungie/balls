use super::*;

#[test]
fn state_remote_opt_precedence() {
    let mut pointer = MasterPointer::default();
    // Neither set → None (caller applies its own default).
    assert_eq!(state_remote_opt(&pointer, None), None);
    // Committed only → committed.
    pointer.state_remote = Some("committed".into());
    assert_eq!(
        state_remote_opt(&pointer, None).as_deref(),
        Some("committed")
    );
    // A local override with no state_remote falls through.
    let bare = LocalConfig::default();
    assert_eq!(
        state_remote_opt(&pointer, Some(&bare)).as_deref(),
        Some("committed")
    );
    // Per-clone override wins over committed.
    let local = LocalConfig {
        state_remote: Some("local".into()),
        ..Default::default()
    };
    assert_eq!(
        state_remote_opt(&pointer, Some(&local)).as_deref(),
        Some("local")
    );
}

#[test]
fn cli_sync_overrides_everything() {
    let p = resolve(false, None, SyncOverride::Sync);
    assert!(p.require_remote);
}

#[test]
fn cli_no_sync_overrides_repo_and_local() {
    let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: Some(true), ..Default::default() }), SyncOverride::NoSync);
    assert!(!p.require_remote);
}

#[test]
fn local_override_beats_repo_default() {
    let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: Some(false), ..Default::default() }), SyncOverride::Unset);
    assert!(!p.require_remote);
    assert!(!p.from_repo_default);
}

#[test]
fn unset_local_falls_through_to_repo_default() {
    let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: None, ..Default::default() }), SyncOverride::Unset);
    assert!(p.require_remote);
    assert!(p.from_repo_default);
}

#[test]
fn no_local_file_falls_through_to_repo_default() {
    let p = resolve(true, None, SyncOverride::Unset);
    assert!(p.require_remote);
    assert!(p.from_repo_default);
}

#[test]
fn off_by_default_when_nothing_set() {
    let p = resolve(false, None, SyncOverride::Unset);
    assert!(!p.require_remote);
    assert!(!p.from_repo_default);
}

#[test]
fn from_repo_default_false_when_local_explicitly_matches() {
    // Local explicitly says true; not "inherited from repo".
    let p = resolve(true, Some(&LocalConfig { require_remote_on_claim: Some(true), ..Default::default() }), SyncOverride::Unset);
    assert!(p.require_remote);
    assert!(!p.from_repo_default);
}

#[test]
fn review_resolver_reads_review_field() {
    // Repo default off, local override on: review picks up the
    // review-specific field, not claim's.
    let local = LocalConfig {
        require_remote_on_claim: None,
        require_remote_on_review: Some(true),
        require_remote_on_close: None,
        ..Default::default()
    };
    let p = resolve_review(false, Some(&local), SyncOverride::Unset);
    assert!(p.require_remote);
    // Claim resolver reading the same local config sees nothing.
    let p = resolve(false, Some(&local), SyncOverride::Unset);
    assert!(!p.require_remote);
}

#[test]
fn close_resolver_cli_overrides_repo_and_local() {
    let local = LocalConfig {
        require_remote_on_close: Some(true),
        ..Default::default()
    };
    let p = resolve_close(true, Some(&local), SyncOverride::NoSync);
    assert!(!p.require_remote);
}
