.PHONY: format lint test fuzz-smoke wasm web serve build check

format:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

fuzz-smoke:
	cargo run --manifest-path fuzz/Cargo.toml --bin parse_note -- -runs=1000 -max_len=65536

wasm:
	cargo check -p memoryfs-wasm --target wasm32-unknown-unknown

web:
	wasm-pack build crates/memoryfs-wasm --target web --out-dir ../../web/pkg --release

serve: web
	python3 -m http.server 4173

build:
	cargo build --workspace --release

check: lint test wasm build
