+++
title = "Decompose mutate cluster: mutate.rs (276) + mutate_build.rs (262)"
created = 1781589010
updated = 1781589010
parent = "bl-fac7"
+++
mutate.rs: lift base_change + Authored + command + one_positional to mutate_author.rs (~110 lines out → ~165). mutate_build.rs: lift per-verb guards (forbid_* family + shapes, 178-262, ~85 lines) to mutate_guards.rs. Both re-exported. Sequence AFTER Tier 1 (--remote helper touches mutate_args).