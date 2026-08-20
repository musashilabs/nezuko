# ============================================================================
# nezuko — Makefile
# ----------------------------------------------------------------------------
# One place for every dev / CI / release workflow.
# Run `make help` for the menu.
# ============================================================================

# Bash with strict mode so a failing pipe fails the target.
SHELL       := bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help

CARGO       ?= cargo
CARGO_FLAGS ?=
FEATURES    ?=
RUSTFLAGS   ?= -D warnings

# color
GREEN  := \033[0;32m
YELLOW := \033[0;33m
BLUE   := \033[0;34m
RESET  := \033[0m

# ----------------------------------------------------------------------------
# Meta
# ----------------------------------------------------------------------------
.PHONY: help
help: ## Print this help
	@awk 'BEGIN {FS = ":.*##"; printf "\n$(BLUE)Usage:$(RESET)\n  make $(YELLOW)<target>$(RESET)\n\n$(BLUE)Targets:$(RESET)\n"} \
		/^[a-zA-Z_-]+:.*?##/ { printf "  $(YELLOW)%-18s$(RESET) %s\n", $$1, $$2 } \
		/^##@/ { printf "\n$(BLUE)%s$(RESET)\n", substr($$0, 5) }' $(MAKEFILE_LIST)


.PHONY: hooks
hooks: ## Point git at .githooks/ (version-controlled hooks)
	@chmod +x .githooks/*
	@# make the +x bit persist in git's index so fresh clones inherit it
	@for h in .githooks/*; do \
		if git ls-files --error-unmatch "$$h" >/dev/null 2>&1; then \
			git update-index --chmod=+x "$$h" 2>/dev/null || true; \
		fi; \
	done
	git config core.hooksPath .githooks
	@echo "$(GREEN)✓ git hooks wired to .githooks/$(RESET)"

.PHONY: setup
setup: hooks ## Alias for hooks — quick one-shot after cloning
	@echo "$(GREEN)✓ setup complete — try: git commit$(RESET)"

##@ Setup

.PHONY: bootstrap
bootstrap: ## Install all dev tooling and wire up git hooks
	@echo "$(GREEN)→ installing dev tools$(RESET)"
	rustup component add rustfmt clippy rust-src
	@if ! command -v cargo-binstall >/dev/null; then \
		curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash ; \
	fi
	cargo binstall --no-confirm \
		cargo-nextest \
		cargo-deny    \
		cargo-audit   \
		cargo-outdated \
		cargo-machete \
		cargo-watch   \
		cargo-llvm-cov \
		cargo-chef
	@$(MAKE) hooks
	@echo "$(GREEN)✓ bootstrap complete$(RESET)"

.PHONY: hooks
hooks:
	git config core.hooksPath .githooks
	chmod +x .githooks/*
	@echo "$(GREEN)✓ git hooks wired to .githooks/$(RESET)"

##@ Build

.PHONY: build
build: ## Debug build
	$(CARGO) build $(CARGO_FLAGS)

.PHONY: release
release: ## Release build
	$(CARGO) build --release $(CARGO_FLAGS)

.PHONY: check
check: ## Fast type-check without codegen
	$(CARGO) check --all-targets --all-features

.PHONY: clean
clean: ## Wipe target/
	$(CARGO) clean

##@ Quality

.PHONY: fmt
fmt: ## Format the workspace
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Fail if anything is not formatted
	$(CARGO) fmt --all -- --check

.PHONY: clippy
clippy: ## Lint (warnings are errors)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --all-targets --all-features -- -D warnings

.PHONY: fix
fix: ## Auto-fix clippy and rustfmt
	$(CARGO) fix --all-targets --all-features --allow-dirty --allow-staged
	$(CARGO) clippy --fix --all-targets --all-features --allow-dirty --allow-staged
	$(CARGO) fmt --all

##@ Security

.PHONY: audit
audit: ## Scan Cargo.lock against RustSec advisories
	$(CARGO) audit --deny warnings

.PHONY: deny
deny: ## Check advisories + licenses + bans + sources
	$(CARGO) deny check

.PHONY: outdated
outdated: ## Show dependencies with newer versions available
	$(CARGO) outdated --workspace --root-deps-only

.PHONY: machete
machete: ## Find unused dependencies
	$(CARGO) machete

##@ Test

.PHONY: test
test: ## Run tests via nextest
	$(CARGO) nextest run --all-features

.PHONY: test-doc
test-doc: ## Run doc-tests (nextest doesn't run these)
	$(CARGO) test --doc --all-features

.PHONY: test-all
test-all: test test-doc ## Run every test target

.PHONY: coverage
coverage:
	$(CARGO) llvm-cov nextest --all-features --html
	@echo "$(GREEN)→ open target/llvm-cov/html/index.html$(RESET)"

##@ Perf

.PHONY: bench
bench:
	$(CARGO) bench --all-features

.PHONY: flamegraph
flamegraph:
	$(CARGO) flamegraph --profile profiling --bench runtime

##@ Docs

.PHONY: doc
doc:
	RUSTDOCFLAGS="--cfg docsrs -D warnings" $(CARGO) doc --all-features --no-deps --open

##@ CI Gate

.PHONY: ci
ci: fmt-check clippy deny audit test test-doc ## Everything CI runs, locally
	@echo "$(GREEN)✓ CI gate passed$(RESET)"

.PHONY: watch
watch: 
	$(CARGO) watch -x check -x 'nextest run'

