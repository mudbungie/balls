+++
title = "Split src/plugin_tests.rs (298/300) before the next edit trips the line cap"
created = 1781036113
updated = 1781036113
parent = "bl-72a8"
priority = 3
+++
src/plugin_tests.rs sits at 298 lines against the 300-line pre-commit cap — two lines of headroom; the next test added to the plugin layer trips the hook mid-task for whoever happens to touch it. Pre-emptively split per the established convention: lift a cohesive slice to a sibling module (plugin_tests/<topic>.rs or a second sibling *_tests.rs) and re-export. Nearby-but-fine for awareness: src/tracker/prime.rs 291, src/lifecycle_tests.rs 290, src/plugin.rs 282 — split only if you're already in there.

**Why:** the cap is a good gate but a bad surprise; paying the split now, in a dedicated ball, beats paying it inside an unrelated claimed task at commit time.