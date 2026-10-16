.DEFAULT_GOAL := build

.PHONY: update build test install clean rust-doc uml

BASH_COMPLETION := target/asb.bash
ZSH_COMPLETION := target/_asb
AUTO_UML_VERSION := 0.7.2
AUTO_UML_ROOT := target/tools/auto-uml
AUTO_UML := $(AUTO_UML_ROOT)/bin/auto-uml
UML_DOT := target/asb-uml.dot
UML_SVG := target/asb-uml.svg

update:
	git pull --ff-only origin main

build:
	cargo build --locked
	$(MAKE) rust-doc uml
	target/debug/asb --__generate-completion bash > "$(BASH_COMPLETION)"
	target/debug/asb --__generate-completion zsh > "$(ZSH_COMPLETION)"

rust-doc:
	cargo doc --locked --workspace --no-deps

$(AUTO_UML):
	mkdir -p "$(AUTO_UML_ROOT)"
	cargo install auto-uml --version "$(AUTO_UML_VERSION)" --locked --root "$(AUTO_UML_ROOT)"

uml: $(AUTO_UML)
	$(AUTO_UML) --lang rust --source-code crates --format graphviz --destination "$(UML_DOT)"
	@if command -v dot >/dev/null 2>&1; then \
		dot -Tsvg "$(UML_DOT)" -o "$(UML_SVG)"; \
	else \
		python3 scripts/render_dot_svg.py "$(UML_DOT)" "$(UML_SVG)"; \
	fi

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
