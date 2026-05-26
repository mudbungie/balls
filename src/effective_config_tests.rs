use super::*;
use crate::layered_fields::{Integrate, IntegrateMode, ReviewBlock};

fn project_with(integrate: Option<Integrate>) -> ProjectConfig {
    ProjectConfig {
        integrate,
        ..Default::default()
    }
}

fn repo_with(integrate: Option<Integrate>) -> RepoJson {
    RepoJson {
        integrate,
        ..Default::default()
    }
}

fn clone_with(integrate: Option<Integrate>) -> CloneJson {
    CloneJson {
        integrate,
        ..Default::default()
    }
}

#[test]
fn all_three_layers_set_clone_wins() {
    // SPEC §14.8 three-way precedence: clone beats repo beats
    // project. Set distinct values at each layer; the clone wins.
    let project = project_with(Some(Integrate {
        mode: IntegrateMode::Direct,
    }));
    let repo = repo_with(Some(Integrate {
        mode: IntegrateMode::Direct,
    }));
    let clone = clone_with(Some(Integrate {
        mode: IntegrateMode::ForgePr,
    }));
    let e = EffectiveConfig::resolve(&project, &repo, Some(&clone));
    assert_eq!(e.integrate.mode, IntegrateMode::ForgePr);
}

#[test]
fn only_repo_set_repo_wins() {
    let project = ProjectConfig::default();
    let repo = repo_with(Some(Integrate {
        mode: IntegrateMode::ForgePr,
    }));
    let e = EffectiveConfig::resolve(&project, &repo, None);
    assert_eq!(e.integrate.mode, IntegrateMode::ForgePr);
}

#[test]
fn only_project_set_project_wins() {
    let project = project_with(Some(Integrate {
        mode: IntegrateMode::ForgePr,
    }));
    let repo = RepoJson::default();
    let e = EffectiveConfig::resolve(&project, &repo, None);
    assert_eq!(e.integrate.mode, IntegrateMode::ForgePr);
}

#[test]
fn no_layer_set_falls_through_to_default() {
    let project = ProjectConfig::default();
    let repo = RepoJson::default();
    let e = EffectiveConfig::resolve(&project, &repo, None);
    assert_eq!(e.integrate, Integrate::default());
    assert_eq!(e.review, ReviewBlock::default());
}

#[test]
fn require_remote_three_way_precedence() {
    // SPEC §14.8: same precedence rule on the bool fields.
    let project = ProjectConfig {
        require_remote_on_claim: Some(false),
        ..Default::default()
    };
    let repo = RepoJson {
        require_remote_on_claim: Some(true),
        ..Default::default()
    };
    let clone = CloneJson {
        require_remote_on_claim: Some(false),
        ..Default::default()
    };
    let e = EffectiveConfig::resolve(&project, &repo, Some(&clone));
    assert!(!e.require_remote_on_claim); // clone wins
}

#[test]
fn require_remote_repo_when_clone_silent() {
    let project = ProjectConfig {
        require_remote_on_review: Some(false),
        ..Default::default()
    };
    let repo = RepoJson {
        require_remote_on_review: Some(true),
        ..Default::default()
    };
    let e = EffectiveConfig::resolve(&project, &repo, None);
    assert!(e.require_remote_on_review);
}

#[test]
fn require_remote_project_when_repo_silent() {
    let project = ProjectConfig {
        require_remote_on_close: Some(false),
        ..Default::default()
    };
    let repo = RepoJson::default();
    let e = EffectiveConfig::resolve(&project, &repo, None);
    assert!(!e.require_remote_on_close);
}

#[test]
fn require_remote_default_when_no_layer_set() {
    let project = ProjectConfig::default();
    let repo = RepoJson::default();
    let e = EffectiveConfig::resolve(&project, &repo, None);
    assert!(e.require_remote_on_claim);
    assert!(e.require_remote_on_review);
    assert!(e.require_remote_on_close);
}

#[test]
fn review_gate_command_propagates() {
    let clone = CloneJson {
        review: Some(ReviewBlock {
            gate_command: Some("make ci".into()),
        }),
        ..Default::default()
    };
    let e = EffectiveConfig::resolve(&ProjectConfig::default(), &RepoJson::default(), Some(&clone));
    assert_eq!(e.review.gate_command.as_deref(), Some("make ci"));
}

#[test]
fn repo_only_auto_fetch_carries_through() {
    let repo = RepoJson {
        auto_fetch_on_ready: false,
        ..Default::default()
    };
    let e = EffectiveConfig::resolve(&ProjectConfig::default(), &repo, None);
    assert!(!e.auto_fetch_on_ready);
}

#[test]
fn repo_only_auto_fetch_default_when_repo_uses_default_value() {
    // RepoJson::default() sets auto_fetch_on_ready = true (the
    // built-in default). The merger reads it directly — no
    // project layer for repo-only fields.
    let e = EffectiveConfig::resolve(&ProjectConfig::default(), &RepoJson::default(), None);
    assert!(e.auto_fetch_on_ready);
}

#[test]
fn clone_overrides_repo_only_auto_fetch() {
    // Even though auto_fetch_on_ready is a repo-only field (no
    // project layer), clone.json can still override it. SPEC §6.5
    // table: clone column says "optional override" for every
    // repo-owned field.
    let repo = RepoJson {
        auto_fetch_on_ready: true,
        ..Default::default()
    };
    let clone = CloneJson {
        auto_fetch_on_ready: Some(false),
        ..Default::default()
    };
    let e = EffectiveConfig::resolve(&ProjectConfig::default(), &repo, Some(&clone));
    assert!(!e.auto_fetch_on_ready);
}

#[test]
fn clone_overrides_repo_stale_threshold() {
    let repo = RepoJson {
        stale_threshold_seconds: 100,
        ..Default::default()
    };
    let clone = CloneJson {
        stale_threshold_seconds: Some(42),
        ..Default::default()
    };
    let e = EffectiveConfig::resolve(&ProjectConfig::default(), &repo, Some(&clone));
    assert_eq!(e.stale_threshold_seconds, 42);
}

#[test]
fn worktree_dir_clone_wins_then_repo_then_none() {
    let repo = RepoJson {
        worktree_dir: Some("/repo/wt".into()),
        ..Default::default()
    };
    // Clone wins:
    let clone = CloneJson {
        worktree_dir: Some("/clone/wt".into()),
        ..Default::default()
    };
    assert_eq!(
        EffectiveConfig::resolve(&ProjectConfig::default(), &repo, Some(&clone)).worktree_dir,
        Some("/clone/wt".into())
    );
    // Repo wins when clone is silent:
    assert_eq!(
        EffectiveConfig::resolve(&ProjectConfig::default(), &repo, None).worktree_dir,
        Some("/repo/wt".into())
    );
    // Neither set → None:
    assert_eq!(
        EffectiveConfig::resolve(
            &ProjectConfig::default(),
            &RepoJson::default(),
            None,
        )
        .worktree_dir,
        None
    );
}

#[test]
fn protected_main_three_layer_precedence() {
    let repo = RepoJson {
        protected_main: false,
        ..Default::default()
    };
    let clone = CloneJson {
        protected_main: Some(true),
        ..Default::default()
    };
    let e = EffectiveConfig::resolve(&ProjectConfig::default(), &repo, Some(&clone));
    assert!(e.protected_main);
}

#[test]
fn defaults_match_documented_built_ins() {
    // §6.5 built-in defaults: integrate=Direct, review.gate_command=None,
    // require_remote_*=true, auto_fetch_on_ready=true,
    // stale_threshold_seconds=86400, worktree_dir=None,
    // protected_main=true.
    let e = EffectiveConfig::resolve(&ProjectConfig::default(), &RepoJson::default(), None);
    assert_eq!(e.integrate.mode, IntegrateMode::Direct);
    assert_eq!(e.review.gate_command, None);
    assert!(e.require_remote_on_claim);
    assert!(e.require_remote_on_review);
    assert!(e.require_remote_on_close);
    assert!(e.auto_fetch_on_ready);
    assert_eq!(e.stale_threshold_seconds, 86_400);
    assert_eq!(e.worktree_dir, None);
    assert!(e.protected_main);
}
