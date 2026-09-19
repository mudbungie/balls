#!/usr/bin/env bash
# leak rules (from yog, via the rust-bootstrap template; ported here under
# bl-816b) — the TABLE, and only the table. `scripts/leak-scan.sh` is the
# mechanism that runs it; this file is what counts as a leak. Split out under
# bl-167d so both halves stay inside the 300-line cap and so "what may not be
# committed" reads as one list.
#
# Sourced, never executed.
#
#   PATTERN — ERE matched with `grep -o`, so a hit is the offending text.
#   EXCEPT  — dropped if the matched text matches this ERE, ANCHORED AT THE
#             START of the match (there is nowhere else to anchor: the hit line
#             is `path:line:match`, so `^` would mean the path, while `$` does
#             mean the end of the match, because the match ends the line).
#             Write a leading `.*` where you mean "contains".
#   WHY     — printed with the finding; it must say what to DO.
#
# ONE INVARIANT GOVERNS THIS FILE: **no pattern here may match this file.**
# The scanner scans itself and this table — bl-167d deleted the exemption that
# made the file the rules live in the one place a leak could not be seen — so a
# rule whose literal text is also an instance of itself would flag the table
# forever. Exactly one ever was: PuTTY's key banner, the only bare literal with
# nothing after it. It is written `Fil[e]` for that reason, the same idiom as
# `ps | grep '[s]shd'`. Every other pattern is self-immune already, because its
# literal prefix is followed by a bracket expression or an alternation bar.
#
# There is no per-rule path exemption and no allowlist. There was one — a SKIP
# prefix for `docs/drive-logs/`, whose files were operator-home paths by the
# hundred — and bl-244f burned those logs rather than keep the carve-out they
# justified. Every rule applies to every tracked file. Add an escape back only
# when a rule is wrong, and fix the rule instead if you can.

declare -A PATTERN EXCEPT WHY

RULES=(private-key vendor-token credential-assignment ipv4-routable
       ipv6-address mac-address home-path personal-email quoted-dialogue
       session-artifact)

PATTERN[private-key]='-----BEGIN( [A-Z0-9]+)* PRIVATE KEY-----|PuTTY-User-Key-Fil[e]'
WHY[private-key]='a private key. Remove it, then rotate it — it is burned.'

PATTERN[vendor-token]='sk-ant-[A-Za-z0-9_-]{16,}|sk-(proj-)?[A-Za-z0-9]{32,}|gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|(AKIA|ASIA)[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|xox[abprs]-[A-Za-z0-9-]{10,}|glpat-[A-Za-z0-9_-]{20}|eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}'
WHY[vendor-token]='a live API token. Remove it, then revoke it at the vendor.'

PATTERN[credential-assignment]="(pass(word|wd|phrase)?|secret|token|api[_-]?key|apikey|credential|authorization)['\"]?[[:space:]]*[:=][[:space:]]*['\"][^'\"]{8,}['\"]"
# Two escapes. The first is the declared placeholder vocabulary. The second is
# structural and does the real work: a multi-word ALL-ALPHABETIC value is
# prose, not a secret — `"credential":"not required"` is a status field, and
# `bz --list-providers --json` emits four of them (src/config_edit/brazen/
# providers/tests.rs). A secret that survives it must carry a digit, a symbol,
# or no space at all, which every real one does.
EXCEPT[credential-assignment]=".*(REDACTED|redacted|<[^>]*>|\\\$\{|\\\$[A-Z_]+|example|placeholder|dummy|fake|changeme|xxxx|TODO|\.\.\.|test|invalid)|.*['\"][A-Za-z]+( [A-Za-z]+)+['\"]\$"
WHY[credential-assignment]='a credential assigned a real-looking value. Use a placeholder ("<redacted>", "${VAR}", "example") if it is illustrative.'

PATTERN[ipv4-routable]='\b((25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.){3}(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\b'
# Loopback, the unspecified and broadcast addresses, and the three RFC5737
# documentation ranges. Everything else — RFC1918 included — is network
# topology, and internal topology is exactly the kind of thing this scan is
# for.
EXCEPT[ipv4-routable]='127\.|0\.0\.0\.0|255\.255\.255\.255|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.'
WHY[ipv4-routable]='a routable IP address. Use a documentation range (192.0.2.x, 198.51.100.x, 203.0.113.x) or loopback.'

PATTERN[ipv6-address]='\b[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){7}\b'
EXCEPT[ipv6-address]='2001:0*[dD][bB]8:'
WHY[ipv6-address]='an IPv6 address. Use the 2001:db8:: documentation prefix.'

PATTERN[mac-address]='\b([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}\b'
WHY[mac-address]='a MAC address — a permanent hardware identifier for one machine.'

# Every home root on every platform, NOT the scanning account's own `$HOME`
# (bl-167d). The old rule was built from `$HOME` at scan time, which read as
# identity-free and was in fact runner-dependent: CI runs as `runner`, so it
# could not see the author's home at all, and no box but the author's could
# see the author's paths. The rule that holds everywhere is the inverse — an absolute
# path under ANY home root is a leak unless its account name is one of the
# house's three synthetic roots (`/home/u`, `/home/op`, `/home/x`, AGENTS.md).
#
# `(^|[^A-Za-z0-9._/-])` is load-bearing: `/home` also occurs as an interior
# path segment with a wholly different meaning — a delivery worktree mirrors
# the repo path under the plugin's territory, so `bl-delivery/home/u/dev/x`
# is a mirror, not anybody's home — so the rule fires only where `/home/`
# STARTS an absolute path. The trailing boundary is the other half: a
# synthetic root must never match inside a longer account name.
#
# The second alternative is this repo's own: balls keys its store clones on
# the PERCENT-ENCODED repo path (`clones/%2Fhome%2Fu%2Fdev%2Fx`, src/
# encoding.rs), and an encoded home path names the account exactly as the
# plain one does. The account name is bounded by the next `%`, the end, or
# any non-name byte; the synthetic roots are excepted in both spellings.
PATTERN[home-path]='(^|[^A-Za-z0-9._/-])((/home|/Users)/|[A-Za-z]:[\\/][Uu]sers[\\/])[A-Za-z0-9._-]+([^A-Za-z0-9._-]|$)|%2[Ff](home|Users)%2[Ff][A-Za-z0-9._-]+([^A-Za-z0-9._-]|$)'
EXCEPT[home-path]='.?((/home|/Users)/|[A-Za-z]:[\\/][Uu]sers[\\/])(u|op|x)([^A-Za-z0-9._-]|$)|%2[Ff](home|Users)%2[Ff](u|op|x)([^A-Za-z0-9._-]|$)'
WHY[home-path]='an absolute path under a home directory — it names an account, and it is a portability bug besides. Use a synthetic root (/home/u).'

# An address is an identity, and one pasted from a config, a transcript or a
# stack trace names a person who did not publish it. The escape is the reserved
# documentation and test space (RFC2606/RFC6761): example.com/org/net and the
# `.invalid`, `.test`, `.local`, `.localhost` TLDs, anchored at the END of the
# match, so a domain that merely BEGINS with a reserved label cannot borrow
# the escape. No literal identity is excepted here: this tree's LICENSE names
# "ball contributors", so there is no authorship declaration to carry, and the
# template's one such escape (rust-bootstrap) was dropped in the port.
PATTERN[personal-email]='[A-Za-z0-9._%+-]+@[A-Za-z0-9._-]+\.[A-Za-z]{2,}'
EXCEPT[personal-email]='.*@([A-Za-z0-9-]+\.)*(example\.(com|org|net)|invalid|test|local|localhost|localdomain)$'
WHY[personal-email]='an email address. Use a documentation address (u@example.com) or a reserved TLD (t@t.local, t@test.invalid).'

# Pasted conversation. A transcript is content somebody said, and the shape it
# arrives in is a speaker label at the head of a line — bare, quoted, bulleted
# or bolded. The vendor JSON envelope (`"role":"assistant"`) is deliberately
# NOT here: a tool that parses transcripts hits it by the hundred in its own
# tests, and a rule that fires there is turned off within the day. The
# envelope is covered where it is unambiguous instead — see session-artifact.
PATTERN[quoted-dialogue]='^[[:space:]>*_#-]*(Human|Assistant|Claude|ChatGPT|Copilot|Gemini|Codex)[*_]{0,2}[[:space:]]*:[[:space:]]+[^[:space:]]'
WHY[quoted-dialogue]='transcribed dialogue attributed to a speaker. A conversation is private content — cite the conclusion, do not paste the exchange.'

PATTERN[session-artifact]='(msg|toolu|asst|thread|run|req)_[A-Za-z0-9]{20,}|chatcmpl-[A-Za-z0-9]{20,}|"(parentUuid|sessionId|leafUuid|isSidechain)"'
WHY[session-artifact]='a real agent-session artifact (a vendor resource id, or a Claude Code transcript key). Session transcripts are conversation content — they do not belong in a published crate.'

# The path rule. Not a content rule: what is wrong with `.env` is that it
# exists at all, whatever is in it.
FORBIDDEN_PATH='(^|/)(\.env(\..+)?|\.netrc|\.npmrc|\.pypirc|credentials\.json|id_(rsa|dsa|ecdsa|ed25519)(\.pub)?)$|(^|/)(\.ssh|\.aws|\.claude|\.gnupg|\.config/brazen)/|\.(pem|key|p12|pfx|jks|keystore|jsonl|kdbx)$'
WHY[forbidden-path]='a file shape that carries credentials or session state. It should be gitignored, not tracked.'

# The unreadable-content rule (bl-167d). `grep -I` silently SKIPS binary
# files, so before this rule an archive, a database, a PDF, a HAR capture, a
# screenshot or a built executable passed the whole scan by being unreadable —
# the one class of file most likely to carry a dump. Unreadable is now
# rejected, not skipped, and the allowlist is one line of policy: a tracked
# binary must be a DERIVATION the repo can regenerate and check byte for byte.
# balls tracks none, so the allowlist matches nothing: the first tracked
# binary must arrive with its generator, its byte-for-byte test, and a line
# here naming it.
BINARY_ALLOWED='^$'
WHY[binary-content]='content the scanner cannot read — a binary the gate would have to take on faith. Track it only as a regenerable derivation with a byte-for-byte test, named in BINARY_ALLOWED; otherwise do not track it.'

# Every non-comment line of every content-rule fixture must contain this
# marker (bl-167d). No regex can tell a real secret from a fabricated one, so
# the fixture that houses real-SHAPED values carries its falsity in the value
# itself: a rule fixture cannot be updated by pasting something real without
# the author also typing this word onto the line, and a reviewer reading the
# diff sees it. The scanner is what enforces it; see `--self-test`.
FIXTURE_MARKER='notreal'
