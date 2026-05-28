use super::*;
use crate::clone_json::CloneJson;

#[test]
fn cli_sync_overrides_everything() {
    let p = resolve(false, None, SyncOverride::Sync);
    assert!(p.require_remote);
}

#[test]
fn cli_no_sync_overrides_repo_and_local() {
    let p = resolve(
        true,
        Some(&LocalConfig { require_remote_on_claim: Some(true), ..Default::default() }),
        SyncOverride::NoSync,
    );
    assert!(!p.require_remote);
}

#[test]
fn local_override_beats_repo_default() {
    let p = resolve(
        true,
        Some(&LocalConfig { require_remote_on_claim: Some(false), ..Default::default() }),
        SyncOverride::Unset,
    );
    assert!(!p.require_remote);
    assert!(!p.from_repo_default);
}

#[test]
fn unset_local_falls_through_to_repo_default() {
    let p = resolve(
        true,
        Some(&LocalConfig { require_remote_on_claim: None, ..Default::default() }),
        SyncOverride::Unset,
    );
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
    let p = resolve(
        true,
        Some(&LocalConfig { require_remote_on_claim: Some(true), ..Default::default() }),
        SyncOverride::Unset,
    );
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

#[test]
fn sync_override_from_flags_decodes_the_pair() {
    assert_eq!(SyncOverride::from_flags(true, false), SyncOverride::Sync);
    assert_eq!(SyncOverride::from_flags(false, true), SyncOverride::NoSync);
    assert_eq!(SyncOverride::from_flags(false, false), SyncOverride::Unset);
}

#[test]
fn emit_repo_default_sync_notice_skips_when_not_repo_default() {
    // The advisory only fires for the policy-driven case; an explicit
    // CLI/--sync override silences it (the user already knows). The
    // require_remote=false and from_repo_default=false legs both
    // short-circuit the eprintln.
    super::emit_repo_default_sync_notice(super::ClaimPolicy {
        require_remote: false,
        from_repo_default: true,
    });
    super::emit_repo_default_sync_notice(super::ClaimPolicy {
        require_remote: true,
        from_repo_default: false,
    });
}

#[test]
fn from_clone_projects_layered_overrides() {
    // The three `require_remote_on_*` fields ride through; everything
    // else clone.json carries is ignored — those are tracker/repo-
    // scope fields the policy layer doesn't consume.
    let cj = CloneJson {
        require_remote_on_claim: Some(false),
        require_remote_on_review: Some(true),
        require_remote_on_close: None,
        stale_threshold_seconds: Some(99),
        ..Default::default()
    };
    let lc = LocalConfig::from_clone(&cj);
    assert_eq!(lc.require_remote_on_claim, Some(false));
    assert_eq!(lc.require_remote_on_review, Some(true));
    assert_eq!(lc.require_remote_on_close, None);
}
