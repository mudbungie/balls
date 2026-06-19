+++
title = "Fix README drift + stale Makefile from bl-tracker rename"
created = 1781835761
updated = 1781835761
+++
Audit found genuine drift: (1) bl conf and bl import are real verbs missing from the Commands table (conf is even used in README prose); (2) the reproduced seed plugins.toml block omits import.post = [bl-tracker]; (3) executable counts are stale now that bl-chore ships and make install drops it; (4) the Makefile's install-tracker/uninstall targets still reference the pre-rename 'tracker' binary name, which breaks make install (cargo autobins produces target/release/bl-tracker, not tracker). Fix all in README.md + Makefile.