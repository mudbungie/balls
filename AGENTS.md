# AGENTS.md

This repo uses **balls** (`bl`) for task tracking — and builds it. Run `bl --skill` for the
operating guide, then `bl <cmd> --skill` before you run any command; each verb documents its own
flags and semantics there.

Session start is `bl prime --as YOUR_IDENTITY`, then `bl list`. The identity comes from your
harness, never from you — see *Identity* in `README.md` for why.

`docs/architecture.md` (§0–§16) is the frozen design reference and the authority for behavior. Where
it and the code disagree, one of them is wrong: fix that, do not route around it. Never implement a
deviation silently — amend the doc.

`README.md` covers install and the repo bootstrap this file is part of. The gate is below.

## The gate

`make check` is the complete gate: `lint → test → coverage`, where `lint` is `leak-scan → rustdoc
-D warnings → deploy-selftest → clippy -D warnings → 300-line cap → coverage-selftest`, and
`scripts/check` is that sequence itself, carrying the three-word exit `make` cannot (0 pass, 75 no
verdict, other fail — bl-988d). The pre-commit hook (`scripts/pre-commit`) does not run them on this
machine: this laptop does not compile in a gate, and `cargo tarpaulin` / `cargo llvm-cov` are shimmed
here to refuse (ops bl-3166, `~/ops/remote-builds.md` "Phase 2"). The hook is `exec bl-gate`
(userconf): it leak-scans locally, exports `BALLS_TOOLCHAIN`, asks `bl-speculate check` for a
verified verdict on the staged tree, and otherwise has the noodlezoo builder run `scripts/check` and
sign one (`bl-remote-gate`, `~/ops/noodlezoo/docs/builder.md`). Exit 0 is a pass; 1 means the
builder failed the tree (`ssh builder cat /tank/build/out/<sha>/log`); 75 means no verdict — nothing
recorded, commit refused, never `cargo test` instead. `bl-remote-run <target>` runs any make target
on the builder when you want tests before committing (`bl-remote-run lint` is the fast one).

`rust-toolchain.toml` pins the rustc every side resolves; `rustc -V` is the gate half of every
verdict key, so the pin is what makes a builder's verdict land here. `.github/workflows/speculate.yml`
runs `scripts/check` on a GitHub runner and records its own verdict; `ci.yml` runs the same steps on
every push and pull request; nobody restates a step the Makefile defines. Run `make hooks` once per
clone — it seats the stub that execs the committing worktree's `scripts/pre-commit`.

**All tests must pass and coverage must be 100% before anything merges.** It does not matter who
broke the test.

Pointers, not copies — this file names where things live and restates none of them.
