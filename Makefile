.PHONY: web-build web-check build check cargo-check ci ci-test ci-lint

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

ci-test:
	cargo test --all-targets --features mock-capture

ci-lint:
	cargo clippy --all-targets --all-features -- -D warnings

ci: ci-lint ci-test web-check
