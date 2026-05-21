#!/usr/bin/env bash
# Install the balls pre-commit hook into this repository's shared
# hooks dir. Works from any linked worktree and from a bare hub.
#
# The hook is a small stub, not a symlink to a fixed checkout. An
# earlier version pointed the symlink at <git-common-dir>/../scripts,
# i.e. the scripts/ dir of a presumed main checkout beside .git. A
# bare hub has no main checkout — scripts/ lives only inside the
# ephemeral .balls-worktrees/<id>/ worktrees — so that symlink
# dangled permanently and git silently ran no hook at all.
#
# The stub instead resolves the *committing* worktree at commit time
# and execs that worktree's own scripts/pre-commit. Correct for bare
# hubs and ordinary clones alike; a removed worktree never dangles it.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
COMMON_DIR="$(cd "$(git rev-parse --git-common-dir)" && pwd)"
HOOK_DIR="$COMMON_DIR/hooks"
DEST="$HOOK_DIR/pre-commit"

mkdir -p "$HOOK_DIR"

if [[ -e "$DEST" && ! -L "$DEST" ]]; then
    echo "Backing up existing hook to $DEST.backup"
    mv "$DEST" "$DEST.backup"
fi

# Drop any prior install (notably a stale dangling symlink) so the
# write below replaces it cleanly rather than following the link.
rm -f "$DEST"

cat > "$DEST" <<'HOOK'
#!/usr/bin/env bash
# balls pre-commit hook — installed by scripts/install-hooks.sh.
# Delegates to the committing worktree's own scripts/pre-commit,
# resolved at commit time, so the hook is correct in a bare hub
# (no main checkout) and tracks whatever gate script is checked
# out. A worktree without that script — the balls/tasks state
# branch — commits ungated.
set -euo pipefail
hook="$(git rev-parse --show-toplevel)/scripts/pre-commit"
[ -x "$hook" ] || exit 0
exec "$hook"
HOOK
chmod +x "$DEST"

# scripts/pre-commit is the real gate runner; keep it and its
# helpers executable in this worktree.
chmod +x "$ROOT/scripts/pre-commit" \
        "$ROOT/scripts/check-line-lengths.sh" \
        "$ROOT/scripts/check-coverage.sh"

echo "Installed pre-commit hook: $DEST"
echo "  -> delegates to <committing-worktree>/scripts/pre-commit"
echo
echo "This hook will block commits with:"
echo "  - clippy or compiler warnings"
echo "  - any Rust source file >= 300 lines"
echo "  - failing tests"
echo "  - test coverage below 100%"
echo
echo "Requires cargo-tarpaulin. If missing:"
echo "  cargo install cargo-tarpaulin"
