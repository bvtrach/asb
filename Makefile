.DEFAULT_GOAL := build

.PHONY: update build test install clean rust-doc uml

BASH_COMPLETION := target/asb.bash
ZSH_COMPLETION := target/_asb
AUTO_UML_VERSION := 0.7.2
AUTO_UML_ROOT := target/tools/auto-uml
AUTO_UML := $(AUTO_UML_ROOT)/bin/auto-uml
UML_MERMAID := docs/asb-type-diagram.mmd
UML_SVG := docs/asb-type-diagram.svg
UML_RAW := target/asb-type-diagram.raw.mmd

update:
	git pull --ff-only origin main

build:
	cargo build --locked
	$(MAKE) rust-doc uml
	target/debug/asb --__generate-completion bash > "$(BASH_COMPLETION)"
	target/debug/asb --__generate-completion zsh > "$(ZSH_COMPLETION)"

rust-doc:
	cargo doc --locked --workspace --no-deps --document-private-items

$(AUTO_UML):
	mkdir -p "$(AUTO_UML_ROOT)"
	cargo install auto-uml --version "$(AUTO_UML_VERSION)" --locked --root "$(AUTO_UML_ROOT)"

uml: $(AUTO_UML)
	$(AUTO_UML) --lang rust --source-code crates --format mermaid --destination "$(UML_RAW)"
	python3 scripts/normalize_uml.py "$(UML_RAW)" "$(UML_MERMAID)"
	python3 scripts/render_mermaid_svg.py "$(UML_MERMAID)" "$(UML_SVG)"

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
