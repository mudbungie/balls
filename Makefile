.PHONY: build test check install clean

build:
	cargo build --release

test:
	cargo test

check: test
	cargo clippy --all-targets -- -D warnings
	scripts/check-line-lengths.sh
	scripts/check-coverage.sh

install: build
	install -d ~/.local/bin
	install -m 0755 target/release/bl ~/.local/bin/bl
	install -d ~/.local/share/bash-completion/completions
	install -d ~/.local/share/zsh/site-functions
	install -d ~/.local/share/fish/vendor_completions.d
	target/release/bl completions bash > ~/.local/share/bash-completion/completions/bl
	target/release/bl completions zsh > ~/.local/share/zsh/site-functions/_bl
	target/release/bl completions fish > ~/.local/share/fish/vendor_completions.d/bl.fish

clean:
	cargo clean
