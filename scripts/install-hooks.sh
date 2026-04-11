#!/usr/bin/env bash
# Install the balls pre-commit hook into this repository's .git/hooks dir.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
HOOK_DIR="$ROOT/.git/hooks"
SRC="$ROOT/scripts/pre-commit"
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
