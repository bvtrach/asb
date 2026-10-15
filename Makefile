.DEFAULT_GOAL := build

.PHONY: update build test install clean

BASH_COMPLETION := target/asb.bash
ZSH_COMPLETION := target/_asb

update:
	git pull --ff-only origin main

build:
	cargo build --locked
	target/debug/asb completion bash > "$(BASH_COMPLETION)"
	target/debug/asb completion zsh > "$(ZSH_COMPLETION)"

test:
	cargo test --locked

install: build
	mkdir -p "$$HOME/.local/bin"
	mkdir -p "$$HOME/.local/share/bash-completion/completions"
	mkdir -p "$$HOME/.local/share/zsh/site-functions"
	install -m 755 target/debug/asb "$$HOME/.local/bin/asb"
	install -m 644 "$(BASH_COMPLETION)" "$$HOME/.local/share/bash-completion/completions/asb"
	install -m 644 "$(ZSH_COMPLETION)" "$$HOME/.local/share/zsh/site-functions/_asb"

clean:
	cargo clean
