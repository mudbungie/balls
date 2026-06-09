+++
title = "github-issues/forge: host-bind the GitHub token + close token.json chmod TOCTOU [bl-2d6d]"
created = 1780978934
updated = 1780978934
parent = "bl-72a8"
priority = 1
tags = ["security"]
+++
Sibling repo /home/mark/dev/balls-github-plugin. From the bl-2d6d review MED-2 (token exfil via unpinned api_base):

Bind the stored token to the api_base it was set up for. balls-github-shared/src/auth.rs Token gains an api_base field; save_token(auth_dir, api_base, token); load_token(auth_dir, api_base) errors loudly on mismatch ('token set up for X, config now says Y; run auth-setup'). A malicious base.json that redirects api_base then fails closed instead of silently leaking the Bearer token to the attacker host. Update both plugin crates' auth_setup/auth_check/sync/push call sites + tests.

Close the chmod TOCTOU: create token.json via OpenOptions .mode(0o600) so it is born owner-only, not world-readable until restrict() runs.

Note: https-pin deliberately skipped — host-binding closes silent exfil regardless of scheme, and the mock test suite runs over http; binding is the real fix.