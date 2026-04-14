.PHONY: build test check install uninstall clean hooks

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

install: build
	install -d $(BIN_DIR)
	install -m 0755 target/release/bl $(BIN_DIR)/bl
	$(BIN_DIR)/bl completions --install

uninstall:
	-$(BIN_DIR)/bl completions --uninstall
	rm -f $(BIN_DIR)/bl

# Install the repo-local pre-commit hook (line-length + clippy + tests
# + 100% coverage). Run once per clone; not part of `make install`
# because a user installing the binary should not have hooks wired
# into whatever repo they happen to be in.
hooks:
	scripts/install-hooks.sh

clean:
	cargo clean
