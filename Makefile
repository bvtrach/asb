.DEFAULT_GOAL := build

.PHONY: update build test install clean

COMPLETION := target/asb.bash

update:
	git pull --ff-only origin main

build:
	cargo build --locked
	target/debug/asb completion bash > "$(COMPLETION)"

test:
	cargo test --locked

install: build
	mkdir -p "$$HOME/.local/bin"
	mkdir -p "$$HOME/.local/share/bash-completion/completions"
	install -m 755 target/debug/asb "$$HOME/.local/bin/asb"
	install -m 644 "$(COMPLETION)" "$$HOME/.local/share/bash-completion/completions/asb"

clean:
	cargo clean
