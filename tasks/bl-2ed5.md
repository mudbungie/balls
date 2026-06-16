+++
title = "Decompose change.rs (276) → lift Create to change_create.rs"
created = 1781589012
updated = 1781589012
parent = "bl-fac7"
+++
Lift Create impl + its vanished helper (51-128, ~78 lines) to change_create.rs, re-exported. Drops change.rs to ~198. Create is the only impl with a private helper — the natural severable unit. Sequence AFTER Tier 1 (finalize_titled, same file).