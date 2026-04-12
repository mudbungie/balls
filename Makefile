.PHONY: build test check install uninstall clean

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

clean:
	cargo clean
