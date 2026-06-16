+++
title = "Tier 0 — delete dead code: delivered_in, on_word, Protocol::handles"
created = 1781588977
updated = 1781589030
claimant = "Sicknesses"
parent = "bl-fac7"
+++
Pure subtraction, 0 prod callers each (verified by grep):
- delivery_repo.rs:148 delivered_in — pass-through to marked(); §11 query never wired. Delete; fold its tests onto marked() directly.
- reads/record.rs:21 on_word — On=Verb so on_word(x) ≡ x.token(). Delete fn + re-export (reads.rs:44) + import (show.rs:16); inline .token() at record.rs:47 and show.rs:133.
- plugin.rs:95 Protocol::handles — 0 callers; install.rs:250 hand-rolls the same op-check inline (speaks is used, handles isn't). Delete + its test.
Watch for orphan-coverage drop when deleting the associated tests.