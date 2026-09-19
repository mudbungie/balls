#!/usr/bin/env bash
# leak self-test — the regression half of `scripts/leak-scan.sh`, lifted to a
# sibling so both halves stay inside the 300-line cap. Sourced by the scanner,
# never executed; it reaches `scan_rule`, `scan_paths`, `scan_binary`, `scan`
# and the rule table through the sourcing shell. Read the scanner's header
# for what this file proves and why.

# --- self-test -------------------------------------------------------------

# Every non-blank, non-'#' line of a rule's fixture must be flagged BY THAT
# RULE and must carry FIXTURE_MARKER; nothing in the clean fixtures may be
# flagged by anything.
fixture_lines() {
  local rule="$1" fixture="$2" hit="$3" ln fails=0 n=0 content
  # ASK THE INFRASTRUCTURE QUESTION FIRST, AND NAME IT SEPARATELY. `scan_rule`
  # greps with `-I`, which reports NO HITS for a file grep judges binary and
  # says nothing about why — so "this rule matched nothing" and "this file could
  # not be read as text here" would arrive as the same sentence, and only the
  # second is a fault of the box rather than of the gate. A fixture is tracked
  # text; if it does not read as text in this locale, the run has no verdict to
  # give about the rule.
  if ! grep -qI '' "$fixture"; then
    echo "self-test: $fixture could not be read as text under LC_ALL=${LC_ALL:-unset} LANG=${LANG:-unset} — an infrastructure fault, not a dead rule" >&2
    return 1
  fi
  while IFS= read -r ln; do
    [ -n "$ln" ] || continue
    n=$((n + 1))
    content="$(sed -n "${ln}p" "$fixture")"
    if [ "$rule" = forbidden-path ]; then
      grep -qF "$content" <<<"$hit" || {
        echo "self-test: [$rule] line $ln of $fixture was NOT flagged" >&2; fails=1; }
      continue
    fi
    grep -qE ":$ln  \[" <<<"$hit" || {
      echo "self-test: [$rule] line $ln of $fixture was NOT flagged" >&2; fails=1; }
    grep -qi "$FIXTURE_MARKER" <<<"$content" || {
      echo "self-test: [$rule] line $ln of $fixture carries no '$FIXTURE_MARKER' marker — a fixture value must be unmistakably fabricated" >&2
      fails=1; }
  done <<<"$(grep -nvE '^(#|$)' "$fixture" | cut -d: -f1)"
  [ "$n" -gt 0 ] || { echo "self-test: $fixture has no cases" >&2; fails=1; }
  return "$fails"
}

self_test() {
  local rule fixture fails=0 hit p
  for rule in "${RULES[@]}" forbidden-path; do
    fixture="$FIXTURES/$rule.txt"
    if [ ! -f "$fixture" ]; then
      echo "self-test: rule '$rule' has no fixture at $fixture" >&2; fails=1; continue
    fi
    if [ "$rule" = forbidden-path ]; then
      hit="$(grep -vE '^(#|$)' "$fixture" | while IFS= read -r p; do scan_paths "$p"; done)"
    else
      hit="$(scan_rule "$rule" "$fixture")"
    fi
    fixture_lines "$rule" "$fixture" "$hit" || fails=1
  done
  # binary-content owns bytes, not lines: its fixture cannot carry a marker or
  # be read at all, so it is capped instead. 512 bytes is far too small to
  # smuggle a dump through the one file the scanner cannot look inside.
  fixture="$FIXTURES/binary-content.bin"
  [ -n "$(scan_binary "$fixture")" ] || {
    echo "self-test: [binary-content] $fixture was NOT flagged" >&2; fails=1; }
  [ "$(wc -c <"$fixture")" -le 512 ] || {
    echo "self-test: $fixture is over the 512-byte cap on unreadable fixtures" >&2; fails=1; }
  # The false-positive direction, and the half that keeps the gate usable: a
  # near-miss for every rule, none of which may be flagged.
  if ! scan "$FIXTURES/clean.txt"; then
    echo "self-test: $FIXTURES/clean.txt was flagged above — a rule is over-broad" >&2
    fails=1
  fi
  while IFS= read -r p; do
    case "$p" in '#'*|'') continue ;; esac
    [ -z "$(scan_paths "$p")" ] || {
      echo "self-test: clean path '$p' was flagged — forbidden-path is over-broad" >&2; fails=1; }
  done <"$FIXTURES/clean-paths.txt"
  # THE HARNESS MUST NOT BE ABLE TO LIE ABOUT A MATCH, and the head of this
  # file says why. This is the two-direction discipline turned on the harness
  # itself: the fixtures prove a rule still bites, and this proves that the
  # answer a pipeline reports is the answer its reader gave.
  #
  # SCOPE IS WHERE `pipefail` EXISTS, which is bash. A `#!` naming any other
  # interpreter is skipped — a POSIX `/bin/sh` script has neither the option
  # that makes the shape wrong nor the herestring that fixes it. A file with no
  # `#!` is a sourced bash fragment and IS in scope. The fixtures are data, not
  # code.
  #
  # The pattern is written `[|]` for the same reason `leak-rules.sh` writes
  # `Fil[e]` — a check that matched its own text would fire forever.
  local shells=() f piped
  while IFS= read -r f; do
    case "$f" in "$FIXTURES"/*) continue ;; esac
    case "$(head -n1 "$f")" in '#!'*bash*) ;; '#!'*) continue ;; esac
    shells+=("$f")
  done < <(git ls-files 'scripts/*')
  if [ "${#shells[@]}" -eq 0 ]; then
    echo "self-test: enumerated 0 scripts — this check is broken, not the tree." >&2
    fails=1
  else
    piped="$(grep -nE '[|][[:space:]]*grep[[:space:]]+-[A-Za-z]*q' "${shells[@]}" || true)"
    if [ -n "$piped" ]; then
      echo "self-test: a 'grep -q' is fed by a PIPE — under pipefail that reports failure when it MATCHED:" >&2
      printf '%s\n' "$piped" >&2
      echo "self-test: read it from a herestring instead — grep -q PATTERN <<<\"\$subject\" — which has no second process to die." >&2
      fails=1
    fi
  fi
  [ "$fails" -eq 0 ] || exit 1
  echo "leak-scan: self-test OK — ${#RULES[@]} content rules + forbidden-path + binary-content all live, clean fixtures unflagged"
}
