#!/usr/bin/env python3
"""One-shot legacy→greenfield base migrator (architecture.md §16). THROWAWAY.

This is NOT a `bl` verb (a verb for a job that runs once over a handful of known
repos would be the §0 "new verb is a smell"). It is the irreducible format
transform — legacy task JSON → greenfield markdown — and nothing else; every
ongoing concern (XDG bootstrap, worktree re-materialization, remote sync) is
`bl prime`'s idempotent job, so the script ends by handing off to prime (see the
RUNBOOK below).

It owns CORE fields only (the base migrator). Per-plugin legacy state
(`external.<plugin>.*`) is DROPPED here; each plugin's greenfield port re-adopts
its own territory (github-issues adoption is bl-1280, downstream of this).

Governing principle — migrate-clean-or-delink, never guess: transform only what
maps deterministically; delink anything unprovable (a dangling parent is nulled,
a not-yet-ported plugin is left out of the hook schedule).

  LEGACY                         GREENFIELD
  balls/tasks:.balls/tasks/*.json    →  balls/tasks: tasks/<id>.md (TOML+++ / md body)
  main:.balls/config.json            →  balls/config: config/{balls,plugins}.toml

Core field map (§16):
  claimed_by→claimant  created_at→created  updated_at→updated  parent  priority
  tags  description→body(+folded notes)  depends_on:[id]→blockers:[{id,on:claim}]
  type=epic→tags+=epic  status=deferred→tags+=deferred
  epic reciprocal edge: for each LIVE child, add {child,on:claim} to its parent.
  dropped (no core home): id(=filename) status delivered_in branch closed_children
  repo type links external synced_at.

Config map (§16): nearly every legacy knob dissolves (id_length, version,
worktree_dir, protected_main, auto_fetch_on_ready → structural/derived;
stale_threshold → tracker territory). What survives is exactly the greenfield
default seed (tasks_branch=balls/tasks, log_level=info) wiring the two shipped
capabilities (tracker + bl-delivery). The legacy github/github-issues plugins
have no greenfield binary YET (their ports are bl-1280), so they are DELINKED
from the hook schedule — re-add via `bl install` once bl-1280 ships them.

╭─ RUNBOOK (the cutover — bl-0802's human-coordinated step) ────────────────────╮
│ Greenfield uses TWO branches; legacy used balls/tasks for the JSON store, so   │
│ the store name COLLIDES — publishing FORCE-rewrites origin/balls/tasks, which  │
│ breaks every legacy clone the instant it lands. So quiesce first; this script  │
│ REFUSES if any live task is claimed. The box running this IS the founding box: │
│ it migrates into its own greenfield store, then force-publishes that store     │
│ (local and remote then share history, so later syncs fast-forward).            │
│                                                                               │
│  0. Quiesce: every agent merges/closes/unclaims; NO live task claimed.        │
│  1. make install            # greenfield bl + tracker + bl-delivery onto PATH  │
│  2. bl prime --as ID        # founds the XDG substrate: seeds the balls/config │
│                             # landing + an empty balls/tasks store. Config     │
│                             # needs NO migration — the seed already IS the     │
│                             # migrated config (every legacy knob dissolved).   │
│  3. STORE=$(ls -d "${XDG_STATE_HOME:-$HOME/.local/state}"/balls/clones/*/tasks)│
│     python3 scripts/migrate-legacy.py --into "$STORE"                          │
│     git -C "$STORE" add -A && git -C "$STORE" commit -m "balls: migrate tasks" │
│  4. git branch balls-archive origin/balls/tasks      # keep the legacy history │
│     git -C "${STORE%/tasks}/config" push --force origin balls/tasks            │
│     git -C "${STORE%/tasks}/config" push        origin balls/config           │
│  5. bl prime --as ID --remote git@github.com:mudbungie/balls.git ; bl list     │
│                                                                               │
│ Multi-machine caveat: a SECOND machine / invocation path priming fresh founds  │
│ an unrelated orphan store, so the ff-only sync refuses ("unrelated histories").│
│ Joining an established remote store needs a prime that clones/resets from it — │
│ a greenfield follow-up, out of scope here. One invocation path is unaffected.  │
╰───────────────────────────────────────────────────────────────────────────────╯

Usage:
  migrate-legacy.py [--repo PATH] [--legacy-ref REF] [--config-src DIR]
                    [--into STORE] [--out DIR] [--build-refs]
                    [--force] [--dry-run] [--self-test]
"""

import argparse
import calendar
import json
import os
import subprocess
import sys
import tempfile
import time

LANDING_BRANCH = "balls/config"
DEFAULT_TASKS_BRANCH = "balls/tasks"
LEGACY_TASKS = ".balls/tasks"  # dir on the legacy store branch
GITIGNORE = "/config/plugins/bin/\n"


def git(repo, *args, stdin=None, check=True):
    """Run a git command in `repo`, returning stdout (stripped). Bare-repo safe."""
    p = subprocess.run(
        ["git", "-C", repo, *args],
        input=stdin,
        capture_output=True,
        text=True,
    )
    if check and p.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)}: {p.stderr.strip()}")
    return p.stdout.rstrip("\n")


def epoch(iso):
    """ISO-8601 (legacy ...Z, nanosecond fraction) → i64 unix seconds (§3)."""
    return int(calendar.timegm(time.strptime(iso[:19], "%Y-%m-%dT%H:%M:%S")))


def toml_str(s):
    """Render a TOML basic string matching the greenfield serializer's escapes."""
    out = ['"']
    for ch in s:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\r":
            out.append("\\r")
        elif ord(ch) < 0x20:
            out.append(f"\\u{ord(ch):04X}")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def to_markdown(task):
    """Render a greenfield Task dict → `tasks/<id>.md` text (§3 TOML+++ form).

    Field order mirrors the Rust struct so the file round-trips: scalar/array
    keys first, the [[blockers]] table-array last (TOML requires tables after
    bare keys), then the markdown body after the closing fence.
    """
    lines = [
        "+++",
        f"title = {toml_str(task['title'])}",
        f"created = {task['created']}",
        f"updated = {task['updated']}",
    ]
    if task.get("claimant"):
        lines.append(f"claimant = {toml_str(task['claimant'])}")
    if task.get("parent"):
        lines.append(f"parent = {toml_str(task['parent'])}")
    if task.get("priority") is not None:
        lines.append(f"priority = {task['priority']}")
    if task.get("tags"):
        rendered = ", ".join(toml_str(t) for t in task["tags"])
        lines.append(f"tags = [{rendered}]")
    for b in task.get("blockers", []):
        lines += ["", "[[blockers]]", f"id = {toml_str(b['id'])}", f"on = {toml_str(b['on'])}"]
    lines.append("+++")
    return "\n".join(lines) + "\n" + task.get("body", "")


def fold_notes(repo, ref, tid, description):
    """Body = legacy description + the task's notes.jsonl folded in as a section.

    Notes have no greenfield core home; folding them into the free-form body
    (the one place that holds prose) preserves them losslessly rather than
    dropping design history.
    """
    body = (description or "").rstrip()
    raw = git(repo, "show", f"{ref}:{LEGACY_TASKS}/{tid}.notes.jsonl", check=False)
    notes = []
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            n = json.loads(line)
        except json.JSONDecodeError:
            continue
        notes.append(f"- {n.get('ts','')} {n.get('author','')}: {n.get('text','')}")
    if notes:
        if body:
            body += "\n\n"
        body += "## Notes (migrated)\n\n" + "\n".join(notes) + "\n"
    elif body:
        body += "\n"
    return body


def load_legacy(repo, ref):
    """Read every legacy task JSON from `ref` (the store branch), keyed by id."""
    listing = git(repo, "ls-tree", "-r", "--name-only", ref, LEGACY_TASKS)
    tasks = {}
    for path in listing.splitlines():
        if not path.endswith(".json"):
            continue
        raw = git(repo, "show", f"{ref}:{path}")
        t = json.loads(raw)
        tasks[t["id"]] = t
    return tasks


def transform(repo, ref, legacy):
    """Legacy task dicts → {id: greenfield-task-dict}. Skips closed (file-absent
    = resolved, §9); nulls a dangling parent; mints the epic reciprocal edge."""
    live = {tid: t for tid, t in legacy.items() if t.get("status") != "closed"}
    out = {}
    for tid, t in live.items():
        parent = t.get("parent")
        if parent not in live:  # dangling (parent closed/absent) → nulled
            parent = None
        tags = list(t.get("tags") or [])
        if t.get("type") == "epic" and "epic" not in tags:
            tags.append("epic")
        if t.get("status") == "deferred" and "deferred" not in tags:
            tags.append("deferred")
        out[tid] = {
            "title": t.get("title", ""),
            "created": epoch(t["created_at"]),
            "updated": epoch(t["updated_at"]),
            "claimant": t.get("claimed_by"),
            "parent": parent,
            "priority": t.get("priority"),
            "tags": tags,
            "blockers": [{"id": d, "on": "claim"} for d in (t.get("depends_on") or [])],
            "body": fold_notes(repo, ref, tid, t.get("description", "")),
        }
    # Epic reciprocal edge: each LIVE child claim-blocks its (live) parent, so the
    # epic stays blocked until its children resolve — legacy derived this from
    # status; greenfield parent is containment-only (§10) and must mint it.
    for tid, t in out.items():
        p = t["parent"]
        if p and p in out:
            edge = {"id": tid, "on": "claim"}
            if edge not in out[p]["blockers"]:
                out[p]["blockers"].append(edge)
    return out


def guard(repo, legacy, config_ref, force):
    """One-shot preconditions (§16): refuse a re-run (balls/config exists) or a
    migration over in-flight work (any live task claimed). --force overrides."""
    problems = []
    if git(repo, "rev-parse", "--verify", "-q", LANDING_BRANCH, check=False):
        problems.append(f"{LANDING_BRANCH} already exists — looks already migrated")
    claimed = [
        tid for tid, t in legacy.items()
        if t.get("status") != "closed" and t.get("claimed_by")
    ]
    if claimed:
        problems.append(f"live tasks are claimed (quiesce first): {', '.join(sorted(claimed))}")
    if problems and not force:
        for p in problems:
            print(f"refusing: {p}", file=sys.stderr)
        print("(re-run with --force only if you understand the consequence)", file=sys.stderr)
        sys.exit(1)
    return problems


def write_tasks(tdir, tasks):
    """Write tasks/<id>.md for every migrated task into `tdir` (the `tasks/`
    folder of a store checkout or staging tree)."""
    os.makedirs(tdir, exist_ok=True)
    for tid, t in tasks.items():
        with open(os.path.join(tdir, f"{tid}.md"), "w") as f:
            f.write(to_markdown(t))


def write_tree(out, tasks, config_src):
    """Materialize the greenfield two-branch content under `out/`: config/ (the
    seed) and tasks/<id>.md (migrated). Plain files — testable, git-free. Used by
    the publish-first path (--build-refs); the --into path skips config because a
    founding `bl prime` already seeds the landing identically (§16 hand-off)."""
    cfg = os.path.join(out, "config")
    os.makedirs(cfg, exist_ok=True)
    for name in ("balls.toml", "plugins.toml"):
        with open(os.path.join(config_src, name)) as f:
            data = f.read()
        with open(os.path.join(cfg, name), "w") as f:
            f.write(data)
    with open(os.path.join(out, ".gitignore"), "w") as f:
        f.write(GITIGNORE)
    tdir = os.path.join(out, "tasks")
    write_tasks(tdir, tasks)
    open(os.path.join(tdir, ".gitkeep"), "w").close()


def build_refs(repo, out):
    """Build the two orphan staging refs from `out/` via plumbing (bare-repo
    safe, no checkout, no clobber of the live branches). Each branch carries its
    single job (§2 sibling split): balls-config gets config/ + .gitignore,
    balls-tasks gets tasks/. The cutover then pushes refs/migrate/balls-config →
    balls/config and (force) refs/migrate/balls-tasks → balls/tasks."""
    config_commit = git(repo, "commit-tree", subtree(repo, out, ["config", ".gitignore"]),
                        "-m", "balls: migrate config")
    tasks_commit = git(repo, "commit-tree", subtree(repo, out, ["tasks"]),
                       "-m", "balls: migrate tasks")
    git(repo, "update-ref", "refs/migrate/balls-config", config_commit)
    git(repo, "update-ref", "refs/migrate/balls-tasks", tasks_commit)
    return config_commit, tasks_commit


def subtree(repo, out, paths):
    """Write a tree object holding only `paths` from the materialized `out/`,
    via a throwaway index (no checkout — bare-repo safe)."""
    index = os.path.join(out, ".idx")
    env = dict(os.environ, GIT_WORK_TREE=out, GIT_INDEX_FILE=index)
    subprocess.run(["git", "-C", repo, "add", "-A", "--", *paths], env=env, check=True, capture_output=True)
    tree = subprocess.run(
        ["git", "-C", repo, "write-tree"], env=env, check=True, capture_output=True, text=True
    ).stdout.strip()
    os.remove(index)
    return tree


def self_test():
    """Validate the transform against synthetic fixtures (no real repo needed)."""
    legacy = {
        "bl-epic": {"id": "bl-epic", "title": "E", "type": "epic", "status": "open",
                    "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
                    "parent": None, "priority": 1, "tags": [], "depends_on": [], "description": "epic body"},
        "bl-kid": {"id": "bl-kid", "title": "K", "type": "task", "status": "open",
                   "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
                   "parent": "bl-epic", "priority": 2, "tags": ["x"], "depends_on": ["bl-dep"], "description": ""},
        "bl-dead": {"id": "bl-dead", "title": "D", "type": "task", "status": "closed",
                    "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
                    "parent": None, "priority": None, "tags": [], "depends_on": [], "description": ""},
        "bl-orphan": {"id": "bl-orphan", "title": "O", "type": "task", "status": "deferred",
                      "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
                      "parent": "bl-dead", "priority": 3, "tags": [], "depends_on": [], "description": ""},
    }
    # fold_notes hits git; stub it for the pure-transform test.
    global fold_notes
    real = fold_notes
    fold_notes = lambda repo, ref, tid, desc: (desc or "")
    try:
        out = transform(".", "ref", legacy)
    finally:
        fold_notes = real
    assert set(out) == {"bl-epic", "bl-kid", "bl-orphan"}, "closed skipped"
    assert "epic" in out["bl-epic"]["tags"], "type=epic → tag"
    assert "deferred" in out["bl-orphan"]["tags"], "deferred → tag"
    assert out["bl-orphan"]["parent"] is None, "dangling parent nulled"
    assert {"id": "bl-kid", "on": "claim"} in out["bl-epic"]["blockers"], "reciprocal epic edge"
    assert {"id": "bl-dep", "on": "claim"} in out["bl-kid"]["blockers"], "depends_on → blocker"
    assert out["bl-kid"]["created"] == epoch("2026-01-01T00:00:00Z")
    md = to_markdown(out["bl-kid"])
    assert md.startswith("+++\ntitle = \"K\"\n"), md
    assert "[[blockers]]" in md and "tags = [\"x\"]" in md
    assert "tags = " in md.split("[[blockers]]")[0], "tags before blockers (TOML order)"
    print("self-test: OK")


def main():
    ap = argparse.ArgumentParser(description="legacy→greenfield base migrator (§16, throwaway)")
    ap.add_argument("--repo", default=".", help="path to the legacy repo (bare ok)")
    ap.add_argument("--legacy-ref", default=DEFAULT_TASKS_BRANCH, help="legacy store branch")
    ap.add_argument("--config-src", default=None, help="dir holding seed balls.toml/plugins.toml (default: <repo>/default-config or ./default-config)")
    ap.add_argument("--out", default=None, help="output tree dir (default: a temp dir)")
    ap.add_argument("--into", default=None, help="write migrated tasks/*.md straight into this STORE checkout (the founding-box cutover path; config is left to `bl prime`)")
    ap.add_argument("--build-refs", action="store_true", help="also build refs/migrate/balls-{config,tasks}")
    ap.add_argument("--force", action="store_true", help="override the one-shot guards")
    ap.add_argument("--dry-run", action="store_true", help="report what would migrate, write nothing")
    ap.add_argument("--self-test", action="store_true", help="run the transform self-test and exit")
    args = ap.parse_args()

    if args.self_test:
        self_test()
        return

    repo = os.path.abspath(args.repo)
    legacy = load_legacy(repo, args.legacy_ref)
    guard(repo, legacy, LANDING_BRANCH, args.force)
    tasks = transform(repo, args.legacy_ref, legacy)

    live = [t for t in legacy.values() if t.get("status") != "closed"]
    print(f"legacy: {len(legacy)} tasks ({len(live)} live) → greenfield: {len(tasks)} migrated")
    for tid in sorted(tasks):
        t = tasks[tid]
        flags = " ".join(filter(None, [
            f"parent={t['parent']}" if t["parent"] else "",
            f"blockers={len(t['blockers'])}" if t["blockers"] else "",
            f"tags={','.join(t['tags'])}" if t["tags"] else "",
        ]))
        print(f"  {tid}  {t['title'][:50]}  {flags}")
    if args.dry_run:
        print("dry-run: nothing written")
        return

    if args.into:
        tdir = os.path.join(os.path.abspath(args.into), "tasks")
        write_tasks(tdir, tasks)
        print(f"wrote {len(tasks)} migrated tasks → {tdir}")
        print("next: commit the store, then force-push it (see the RUNBOOK)")
        return

    config_src = args.config_src or next(
        d for d in (os.path.join(repo, "default-config"), os.path.join(os.getcwd(), "default-config"))
        if os.path.isdir(d)
    )
    out = args.out or tempfile.mkdtemp(prefix="balls-migrate-")
    os.makedirs(out, exist_ok=True)
    write_tree(out, tasks, config_src)
    print(f"wrote greenfield tree → {out}")
    if args.build_refs:
        cc, tc = build_refs(repo, out)
        print(f"built refs/migrate/balls-config  {cc}")
        print(f"built refs/migrate/balls-tasks   {tc}")
        print("next: see the RUNBOOK at the top of this script (push, then `bl prime`)")


if __name__ == "__main__":
    main()
