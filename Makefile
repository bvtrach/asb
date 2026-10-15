.DEFAULT_GOAL := build

.PHONY: build test clean

build:
	cargo build --locked

test:
	cargo test --locked

clean:
	cargo clean
