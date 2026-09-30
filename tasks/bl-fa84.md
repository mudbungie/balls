+++
title = "github-issues: declare and handle the 'comment' op so a bl comment reaches the mirrored issue"
created = 1790732836
updated = 1790732836
priority = 3
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Code lives in ~/dev/balls-github-plugin (sibling repo; tracked here). balls bl-cca0 (2026-09-25): `bl comment` is an update specialization with its own hook key (comment.post), never update.*. github-issues' protocol ops are {create, update, close, sync}, so a comment's body change reaches the GitHub issue only on the next update. Fix: declare "comment" and treat it exactly like update (same payload: a body rewrite), then wire comment.post in balls-github-plugin's landing right after update.post's position. Verify the plugin's network path works at all first (reported broken 2026-07).