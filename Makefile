.PHONY: build test install clean

build:
	cargo build --release

test:
	cargo test

install: build
	install -d ~/.local/bin
	install -m 0755 target/release/bl ~/.local/bin/bl

clean:
	cargo clean
