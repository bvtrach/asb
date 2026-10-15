.DEFAULT_GOAL := build

.PHONY: update build test install clean

update:
	git pull --ff-only origin main

build:
	cargo build --locked

test:
	cargo test --locked

install:
	mkdir -p "$$HOME/.local/bin"
	install -m 755 target/debug/asb "$$HOME/.local/bin/asb"

clean:
	cargo clean
