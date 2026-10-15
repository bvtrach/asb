.DEFAULT_GOAL := build

.PHONY: build test install clean

build:
	cargo build --locked

test:
	cargo test --locked

install:
	mkdir -p "$$HOME/.local/bin"
	install -m 755 target/debug/asb "$$HOME/.local/bin/asb"

clean:
	cargo clean
