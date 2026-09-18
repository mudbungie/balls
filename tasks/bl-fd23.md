+++
title = "Unreachable-remote fail-open warning dumps git's whole multi-line fetch failure, twice per op"
created = 1789711801
updated = 1789711801
priority = 3
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["tracker"]
+++
Seen live: with the hub down, every op prints the tracker's one-sentence warning followed by five lines of git ('fatal: does not appear to be a git repository / Could not read from remote repository. / Please make sure…'), and the same again for the drift count's fetch. bl-3129 lets raw git survive for non-contention failures, but a paragraph per op is not a diagnostic, it is noise. Keep git's FIRST line (the one that names the cause) and drop the rest.