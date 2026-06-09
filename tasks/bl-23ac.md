+++
title = "github-issues/forge: host-bind the GitHub token + close token.json chmod TOCTOU [bl-2d6d]"
created = 1780978934
updated = 1780980148
claimant = "Liveried"
parent = "bl-72a8"
priority = 1
tags = ["security"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-23ac"
+++
BLOCKED — needs human direction on target. The sibling repo /home/mark/dev/balls-github-plugin is mid-migration: it is bare but its on-disk working tree is STALE (matches commit ceea3e3 'Port github-issues to §6/§7 [bl-613d]', whose commands/ structure a later main commit restructured away), so on-disk files != main's tree; and its tracker is OLD-balls (.balls/config.json with require_remote_on_review) which the current greenfield bl cannot prime. Landing a patch safely needs the user to confirm WHICH tree/branch is live (main's current auth module path) before editing — guessing risks clobbering an in-flight migration (possibly a peer agent's work).

READY-TO-APPLY FIX (host-bind the token + close chmod TOCTOU), against balls-github-shared/src/auth.rs:
- Token struct gains api_base: String alongside token.
- save_token(auth_dir, api_base, token): write via OpenOptions().mode(0o600) so the file is BORN owner-only (closes the write-then-chmod TOCTOU); keep restrict() after for the overwrite-of-legacy-0644 case.
- load_token(auth_dir, api_base) -> errors 'token was set up for X, but config now targets Y; run auth-setup' when stored.api_base != api_base. A rewritten base.json then fails CLOSED instead of leaking the Bearer token to the attacker host.
- Update 8 prod call sites (each load/save adds config.api_base(), which is already in scope next to GithubClient::new): issues+forge crates' auth_setup/auth_check/sync/push; plus ~13 test sites.
- https-pin deliberately skipped: host-binding closes silent exfil regardless of scheme, and the mock test suite runs over http.
- Regression tests: roundtrip-same-base ok; load-refuses-different-base; file-is-0600; overwrite-tightens-loose-file. Target gate is its own make check (100% cov + line cap).