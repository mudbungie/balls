+++
title = "update accepts id= as a preserved extra: shadow id key, lossy bedrock round-trip"
created = 1781067248
updated = 1781068965
claimant = "Rebut"
parent = "bl-72a8"
priority = 3
tags = ["bug"]
+++
SS3: "No id field ... the filename basename is the sole source of truth". But the preserved-extras seam accepts it: `bl update X id=bl-feed` exits 0 and stores a literal id = "bl-feed" line in the frontmatter. The projection then shadows it - `show --json` emits only the path-derived id - so export -> import silently DROPS the stored line: the one lossy key in the SS3/SS9 "lossless bedrock mirror, round-trip-safe" contract.

Adjacent: created= / updated= via the same seam are rejected only by an incidental duplicate-key TOML parse error, not a named reserved-key refusal (title= / claimant= legitimately overwrite the structured fields per the update contract).

Fix: refuse the reserved key set (id, created, updated) in the key=value seam with a named error.