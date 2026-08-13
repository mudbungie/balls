+++
title = "release-binaries resolves its tag with git describe, so it races the tag it is supposed to build and uploads to the PREVIOUS release"
created = 1786599852
updated = 1786599852
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug"]
+++
Found while cutting 0.5.10 (see bl-97b4).

.github/workflows/release-plz.yml, job `release-binaries`:

    tag="${{ github.event.inputs.binaries_tag }}"
    [ -n "$tag" ] || tag="$(git describe --tags --abbrev=0)"
    echo "tag=$tag" >> "$GITHUB_OUTPUT"
    git checkout "$tag"

On the automatic path binaries_tag is empty, so it falls through to git describe. That job's actions/checkout races the v<new> tag the release-plz-release job just pushed, and resolves the PREVIOUS tag. The 0.5.10 run proves it: the job log reads 'Compiling balls v0.5.9' while cutting 0.5.10, and it then OVERWROTE v0.5.9's asset and exited success.

Never worked: v0.5.7 and v0.5.8 have no assets at all, and v0.5.9's lone asset is stamped 2026-08-13T03:37:40Z -- created by the 0.5.10 run, not by v0.5.9's own release on 2026-07-27. Every release since the job was added has attached its binaries to the wrong release, or to none.

The fix is subtraction, not a retry or a fetch --tags: the job already gates on needs.release-plz-release.outputs.releases_created, so the release job's own output carries the authoritative tag. Read it instead of re-deriving it from the git graph. git describe is a guess about a fact the upstream job already knows -- a second representation of one fact, which is exactly what drifted. The workflow_dispatch binaries_tag input stays as the backfill override (it is what recovered v0.5.10 by hand, run 31664550241).

Verify by cutting a release and confirming the new tag's Release carries balls-x86_64-unknown-linux-gnu.tar.gz with no manual dispatch.