.DEFAULT_GOAL := build

.PHONY: update build test install clean rust-doc uml

BASH_COMPLETION := target/asb.bash
ZSH_COMPLETION := target/_asb
AUTO_UML_VERSION := 0.7.2
AUTO_UML_ROOT := target/tools/auto-uml
AUTO_UML := $(AUTO_UML_ROOT)/bin/auto-uml
UML_RAW_DOT := target/asb-type-diagram.raw.dot
UML_DOT := docs/asb-type-diagram.dot
UML_SVG := docs/asb-type-diagram.svg

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
	command -v dot >/dev/null || (echo "Graphviz 'dot' is required for UML generation (install the graphviz package)." >&2; exit 1)
	$(AUTO_UML) --lang rust --source-code crates --format graphviz --destination "$(UML_RAW_DOT)"
	python3 scripts/organize_uml_dot.py "$(UML_RAW_DOT)" "$(UML_DOT)"
	dot -Tsvg -o "$(UML_SVG)" "$(UML_DOT)"

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
