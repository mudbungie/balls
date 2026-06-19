.PHONY: build test check \
	install install-core install-tracker install-delivery install-chore \
	uninstall clean hooks

PREFIX ?= $(HOME)/.local
BIN_DIR := $(PREFIX)/bin

build:
	cargo build --release

test:
	cargo test

check: test
	cargo clippy --all-targets -- -D warnings
	scripts/check-line-lengths.sh
	scripts/check-coverage.sh

# The whole suite: core + every shipped sibling plugin. A core-only install
# leaves `bl` resolving no `tracker`/`bl-delivery` beside it (Edge::resolve
# looks beside the bl binary), so `bl prime` founds stealth-only and federation
# silently never engages. Installing the plugins next to bl is what wires them.
install: install-core install-tracker install-delivery install-chore

# Just the bl core binary + the `balls` alias.
install-core: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl $(BIN_DIR)/bl
	ln -sf bl $(BIN_DIR)/balls

# Each plugin is a sibling binary dropped beside bl in BIN_DIR; that adjacency
# is the whole wiring (§6 subprocess-uniform, resolved by Edge::resolve).
install-tracker: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/tracker $(BIN_DIR)/tracker

install-delivery: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl-delivery $(BIN_DIR)/bl-delivery

# §10 guarded-mint primitive (opt-in). Shipped beside bl like any plugin; wire
# it per checkout with `bl conf prepend claim.post bl-chore`.
install-chore: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl-chore $(BIN_DIR)/bl-chore

uninstall:
	rm -f $(BIN_DIR)/bl $(BIN_DIR)/balls $(BIN_DIR)/tracker $(BIN_DIR)/bl-delivery $(BIN_DIR)/bl-chore

# Install the repo-local pre-commit hook (line-length + clippy + tests
# + 100% coverage). Run once per clone; not part of `make install`
# because a user installing the binary should not have hooks wired
# into whatever repo they happen to be in.
hooks:
	scripts/install-hooks.sh

clean:
	cargo clean
