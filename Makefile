.PHONY: build run dev web-dev web-build web-check \
       cargo-check test lint ci status stop logs clean help

# ─── Day-to-day ──────────────────────────────────────────────

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*##' $(MAKEFILE_LIST) | \
		awk -F ':.*## ' '{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

build: web-build ## Build everything (web first, then Rust binary)
	cargo build

run: build ## Build and run the daemon in the foreground
	cargo run

dev: ## Run the Rust daemon (cargo watch) — rebuilds on src/ changes
	cargo run

web-dev: ## Start the Svelte dev server (hot-reload, proxies /api to :7878)
	npm run dev --prefix web

web-build: ## Build the Svelte frontend to web/dist/
	npm run build --prefix web

# ─── Quick commands (assumes daemon is running) ──────────────

status: ## Show daemon status
	cargo run -- status

stop: ## Stop the running daemon
	cargo run -- stop

logs: ## Tail the daemon log
	tail -f ~/.screencap/screencap.log

# ─── Checks ──────────────────────────────────────────────────

test: ## Run Rust tests (mock-capture, no screen recording needed)
	cargo test --all-targets --features mock-capture

lint: ## Run clippy lints
	cargo clippy --all-targets --all-features -- -D warnings

web-check: ## Svelte type-check
	npm run check --prefix web

cargo-check: ## Cargo type-check (no build)
	cargo check

check: web-build web-check cargo-check ## Full type-check (web + Rust)

ci: lint test web-check ## CI pipeline (lint + test + web-check)

# ─── Housekeeping ────────────────────────────────────────────

clean: ## Remove build artifacts
	cargo clean
	rm -rf web/dist web/.svelte-kit
