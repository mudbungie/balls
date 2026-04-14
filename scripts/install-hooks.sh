#!/usr/bin/env bash
# Install the balls pre-commit hook into this repository's shared
# hooks dir. Works from any linked worktree: uses --git-common-dir
# to find the shared admin dir (where hooks/ actually lives) and
# points the symlink at the MAIN checkout's scripts/pre-commit so
# removing a worktree never leaves a dangling hook target.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
COMMON_DIR="$(git rev-parse --git-common-dir)"
case "$COMMON_DIR" in
    /*) ;;
    *) COMMON_DIR="$ROOT/$COMMON_DIR" ;;
esac
MAIN_ROOT="$(cd "$(dirname "$COMMON_DIR")" && pwd)"
HOOK_DIR="$COMMON_DIR/hooks"
SRC="$MAIN_ROOT/scripts/pre-commit"
DEST="$HOOK_DIR/pre-commit"

mkdir -p "$HOOK_DIR"

if [[ -e "$DEST" && ! -L "$DEST" ]]; then
    echo "Backing up existing hook to $DEST.backup"
    mv "$DEST" "$DEST.backup"
fi

ln -sf "$SRC" "$DEST"
chmod +x "$SRC" \
        "$ROOT/scripts/check-line-lengths.sh" \
        "$ROOT/scripts/check-coverage.sh"

echo "Installed pre-commit hook: $DEST -> $SRC"
echo
echo "This hook will block commits with:"
echo "  - any Rust source file >= 300 lines"
echo "  - test coverage below 100%"
echo
echo "Requires cargo-tarpaulin. If missing:"
echo "  cargo install cargo-tarpaulin"
