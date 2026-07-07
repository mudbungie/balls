+++
title = "Tighten usage-error footer to the usage block + point at --skill; update README for the two-tier skill model"
created = 1783397525
updated = 1783397654
claimant = "Economize"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["docs", "dx"]
+++
# Two follow-ups to bl-c704 (the --skill restructure)

## Why
1. The usage-error footer prints the ENTIRE per-command skill doc on a bad argv
   (maintainer: "too verbose", 2026-07-06). A mis-invocation should surface the
   command's SHAPE, not a screenful of prose.
2. README is stale after bl-c704: `README.md` still calls `bl skill` "the full
   manual" and never mentions `bl --skill` or per-command `bl <cmd> --skill`.

## What
- **skill::usage(verb) -> &'static str**: slice the doc's `usage:` block (from the
  `usage: bl …` line to the next blank line). NO new per-verb data — derived from
  the existing embedded doc, so it can't drift.
- **dispatch.rs footer** (the InvalidInput branch in `run`): print the terse error
  + `skill::usage(verb)` + one pointer line (`run bl <cmd> --skill for flags and
  examples`), replacing the full `skill::command(verb)` dump.
- **README.md**: fix the command table row + the SKILL.md blurb to the two-tier
  model — `bl --skill` canonical / `bl skill` deprecated / `bl <cmd> --skill`
  per-command; `bl help` stays the terse directory.

## Definition of done
100% coverage, all files < 300 lines, full suite green. The footer no longer
dumps the whole doc; the integration test that asserts a usage error surfaces the
flags still passes (the usage line carries them, e.g. create's `[--body B]`).