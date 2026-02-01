.DEFAULT_GOAL := all

.PHONY: install  ## Install dependencies for local development
install:
	uv sync --all-extras
	cargo fetch

.PHONY: deps-update  ## Update all dependencies (Cargo.lock and uv.lock)
deps-update:
	cargo update
	uv lock --upgrade

.PHONY: build-dev  ## Build the development version of the package
build-dev:
	uv run maturin develop

.PHONY: build-prod  ## Build the production version of the package
build-prod:
	uv run maturin develop --release

.PHONY: build-dist  ## Build wheels for distribution (native + Linux via zig)
build-dist:
	uv run maturin build --release
	uv run maturin build --release --zig --target x86_64-unknown-linux-gnu --find-interpreter
	uv run maturin build --release --zig --target aarch64-unknown-linux-gnu --find-interpreter

.PHONY: format  ## Auto-format code
format:
	uv run ruff check --fix .
	uv run ruff format .
	cargo fmt

.PHONY: lint  ## Lint code (check mode)
lint:
	uv run ruff check .
	uv run ruff format --check .
	uv run mypy .
	cargo fmt --check
	cargo clippy -- -D warnings

.PHONY: test  ## Run tests
test:
	uv run pytest

.PHONY: bench  ## Run performance benchmarks (RUNS=3 by default)
bench:
	@RUNS=$(or $(RUNS),3) ./performance/bench.sh

.PHONY: all  ## Format, build, lint, and test (local development)
all: format build-dev lint test

.PHONY: ci  ## Build, lint, and test (CI mode, no auto-formatting)
ci: build-dev lint test

.PHONY: clean  ## Clear local caches and build artifacts
clean:
	rm -rf target/wheels
	rm -rf `find . -name __pycache__`
	rm -rf .pytest_cache
	rm -rf .mypy_cache
	rm -rf .ruff_cache

.PHONY: help  ## Display this message
help:
	@grep -E '^.PHONY: .*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ".PHONY: |## "}; {printf "\033[36m%-19s\033[0m %s\n", $$2, $$3}'
