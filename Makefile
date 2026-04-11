.PHONY: web-build web-check build check cargo-check

# Build order is REQUIRED: compile the Svelte frontend first, then compile Rust.
web-build:
	npm run build --prefix web

web-check:
	npm run check --prefix web

cargo-check:
	cargo check

build: web-build
	cargo build

check: web-build web-check cargo-check
