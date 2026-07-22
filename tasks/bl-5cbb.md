+++
title = "Release-plz workflow header undercounts jobs: says Three, describes only 3 of 4"
created = 1784699528
updated = 1784699528
claimant = "Junctions-ballsdoc"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-2c68"
on = "close"
+++
The header comment in .github/workflows/release-plz.yml line 3 reads 'Build-gated, hands-off release. Three jobs:' but a fourth job, prune-release-branches, was appended in bl-b602. Fix the count and add a fourth bullet in the same style.