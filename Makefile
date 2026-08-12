.PHONY: build test check doc \
	install install-core install-tracker install-delivery install-chore \
	uninstall clean hooks

PREFIX ?= $(HOME)/.local
BIN_DIR := $(PREFIX)/bin

build:
	cargo build --release

test:
	cargo test

check: test doc
	cargo clippy --all-targets -- -D warnings
	scripts/check-line-lengths.sh
	scripts/check-coverage.sh

# THE blessed doc build (bl-3d09) — the only invocation this repo's docs are
# guaranteed warning-clean under, so it is the one to run and the one to trust.
#
#   --document-private-items  the docs are a connected map: a module note links
#                             to the private helper carrying the reasoning, so
#                             the private items must be in the rendered graph.
#   -D warnings               every rustdoc warning is an error. A broken link is
#                             a build failure, not a line lost in the scroll —
#                             which is the whole point (68 private-link warnings
#                             used to bury the real ones).
doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

# The whole suite: core + every shipped sibling plugin. A core-only install
# leaves `bl` resolving no `bl-tracker`/`bl-delivery` beside it (Edge::resolve
# looks beside the bl binary), so `bl prime` founds stealth-only and federation
# silently never engages. Installing the plugins next to bl is what wires them.
install: install-core install-tracker install-delivery install-chore install-speculate

# Just the bl core binary + the `balls` alias.
install-core: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl $(BIN_DIR)/bl
	ln -sf bl $(BIN_DIR)/balls

# Each plugin is a sibling binary dropped beside bl in BIN_DIR; that adjacency
# is the whole wiring (§6 subprocess-uniform, resolved by Edge::resolve).
install-tracker: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl-tracker $(BIN_DIR)/bl-tracker

install-delivery: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl-delivery $(BIN_DIR)/bl-delivery

# §10 guarded-mint primitive (opt-in). Shipped beside bl like any plugin; wire
# it per checkout with `bl conf prepend claim.post bl-chore`.
install-chore: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl-chore $(BIN_DIR)/bl-chore

# Verdict-cache edge (bl-1263, design bl-24e7). Consulted by scripts/pre-commit
# via PATH; fail-open, so installing it only ever removes redundant gate runs.
install-speculate: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl-speculate $(BIN_DIR)/bl-speculate

uninstall:
	rm -f $(BIN_DIR)/bl $(BIN_DIR)/balls $(BIN_DIR)/bl-tracker $(BIN_DIR)/bl-delivery $(BIN_DIR)/bl-chore

# Install the repo-local pre-commit hook (line-length + clippy + tests
# + 100% coverage). Run once per clone; not part of `make install`
# because a user installing the binary should not have hooks wired
# into whatever repo they happen to be in.
hooks:
	scripts/install-hooks.sh

clean:
	cargo clean
