+++
title = "Decompose reads.rs (284) → lift Catalog/Entry to reads/catalog.rs"
created = 1781589008
updated = 1781590837
claimant = "Sicknesses"
parent = "bl-fac7"
+++
Lift Catalog, Entry + impls (49-125, ~77 lines) to reads/catalog.rs, pub(crate) use back. Leaves Flags/Reach/run/render/Style ~207; if under-200 wanted also move Style (245-276) to reads/style.rs. Sequence AFTER Tier 0 (on_word, same file) and Tier 1 (status table, same cluster).