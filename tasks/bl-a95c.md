+++
title = "github-issues plugin aborts every mutating balls op on a network failure (should fail-open)"
created = 1783490041
updated = 1783490665
claimant = "opus-a95c"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug"]

[[blockers]]
id = "bl-b37e"
on = "close"
+++
The github-issues plugin (code in ~/dev/balls-github-plugin; wired create.post/update.post/close.post/sync.post in the balls landing) exits non-zero when it cannot reach the GitHub API — observed 2026-07-07: 'github-issues: http: error sending request for url (https://api.github.com/repos/mudbungie/balls/issues)' and 'http: error decoding response body'. Per §6 a plugin's non-zero exit ABORTS the whole op (+ reverse rollback), so a transient network blip or an auth/token problem makes bl create / close / update / sync UNUSABLE in every repo the plugin is wired into. The only workaround is unwiring it (currently bypassed on this box via 'bl conf remove <phase> github-issues'; restore in original order: close.post = bl-delivery, github-issues, bl-tracker; create/update.post = github-issues, bl-tracker; sync.post = github-issues).

THE DESIGN QUESTION (not just the outage): should a github-issues MIRROR failure be fatal to a LOCAL op? Creating/closing a ball is local state that already succeeded; only the external GitHub mirror failed. §6 makes every plugin non-zero exit fatal, but that is right for a LOAD-BEARING plugin (delivery: if the squash fails the close must abort) and wrong for a BEST-EFFORT external MIRROR. This is the SAME asymmetry the clock provider already draws (§6/§8): a load-bearing hook aborts, a cosmetic/advisory one fails OPEN. Direction (subtraction — plugin owns its failure mode, no new core mechanism): github-issues should CATCH its own transport errors and exit 0 with a warning on stderr (which balls envelopes into the op log), reconciling on the next successful sync — exactly like clock_provider degrades to the system clock. Rejected alternative: a core 'advisory hook' flag (new config/mechanism, the §0 smell). Open for dialogue: is 'a plugin decides its own fatal-vs-advisory' a plugin-side convention, or does balls need to say anything about it? Cross-repo: ball here, fix in ~/dev/balls-github-plugin.

DOCS (required when implemented): the plugin's own README in ~/dev/balls-github-plugin — document that it fails OPEN on transport errors (exit 0 + a warning to stderr, reconciled on the next successful sync), so a network blip never blocks a local op. If the fail-open-mirror idea generalizes to a stated principle, add one line to balls architecture §6 (a best-effort external-MIRROR plugin exits 0 + warns on external failure, the same fatal-vs-advisory asymmetry §6/§8 already draw for the clock provider); otherwise keep it a plugin-side convention (no core mechanism).