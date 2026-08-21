.PHONY: format lint test wasm build check

format:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

wasm:
	cargo check -p memoryfs-wasm --target wasm32-unknown-unknown

build:
	cargo build --workspace --release

check: lint test wasm build

