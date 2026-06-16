+++
title = "Decompose delivery_repo.rs (296) → lift impl Repo to delivery_repo_acts.rs"
created = 1781589005
updated = 1781589005
parent = "bl-fac7"
+++
Move impl Repo for Project (materialize/release/discard/integration/deliver, ~98 lines) to delivery_repo_acts.rs (impl blocks register on the type, no re-export needed). Leaves Project struct + plumbing ~200. Sequence AFTER Tier 1 git-builder dedup (same file).