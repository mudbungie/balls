# leak-scan fixtures

Every file here is deliberate. `scripts/leak-scan.sh --self-test` reads them:

- `<rule>.txt` — one case per line; EVERY non-comment line must be flagged by
  that rule. Line granularity, not file granularity: it is what proves each
  alternative inside a many-way pattern is still alive.
- `binary-content.bin` — the one case the scanner cannot read. It is checked by
  shape instead: the rule must flag it, and it must stay under 512 bytes, which
  is far too small to smuggle anything through the one file no rule can look
  inside.
- `clean.txt` / `clean-paths.txt` — near-misses that must NOT be flagged by
  anything. A gate that fires on ordinary code gets bypassed, and a bypassed
  gate is no gate.

**Every non-comment line of a `<rule>.txt` must contain `FIXTURE_MARKER`
(`notreal`, declared in `scripts/leak-rules.sh`), and the self-test fails if
one does not.** These files hold real-SHAPED values, and no regex can tell a
real secret from a fabricated one — so the value says which it is. Nothing here
was ever issued; the token bodies match this repo's patterns and nothing else.

This directory is **not** exempt from the tree scan (bl-167d deleted that
exemption, and the one covering the scanner itself). Each fixture is scanned by
every rule EXCEPT the one it is the fixture of — its own rule must flag it,
that is its contract — which is a structural exemption keyed to the file's own
name, not an allowlist: adding a file to it means adding a RULE of that name.
