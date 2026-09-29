# Rust Notes — project Makefile
#
# Run `make` or `make help` to list available targets.

COMPOSE_DOCS := docker compose -f docker-compose.docs.yml

.DEFAULT_GOAL := help

.PHONY: help all \
        check check-rust check-doc-blocks check-docs check-spelling \
        docs-serve docs-build docs-down docs-logs docs-shell docs-clean \
        video-player

## ----------------------------------------------------------------------------
## Help
## ----------------------------------------------------------------------------

help: ## Show this help message
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| sort \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

all: docs-build ## Default build (renders the static docs site)

## ----------------------------------------------------------------------------
## Verificación (lo mismo que corre el CI)
## ----------------------------------------------------------------------------

check: check-rust check-doc-blocks check-docs check-spelling ## Run every check the CI runs

check-rust: ## fmt + clippy + tests of the example workspace (src/)
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

check-doc-blocks: ## Compile every ```rust block in docs/ against the baseline
	python3 scripts/check_doc_blocks.py

check-docs: ## Build the site in strict mode (broken links, missing snippets)
	mkdocs build --strict --site-dir "$$(mktemp -d)"

check-spelling: ## Spell-check docs in Spanish (needs Node; words list in cspell.json)
	npx --yes -p cspell@8 -p @cspell/dict-es-es cspell --no-progress

## ----------------------------------------------------------------------------
## MkDocs documentation
## ----------------------------------------------------------------------------

docs-serve: ## Serve docs with live reload at http://localhost:8000
	$(COMPOSE_DOCS) up --build docs

docs-build: ## Render the static site into ./site
	$(COMPOSE_DOCS) --profile build run --build --rm build

docs-down: ## Stop and remove the docs container
	$(COMPOSE_DOCS) down

docs-logs: ## Tail logs from the running docs container
	$(COMPOSE_DOCS) logs -f docs

docs-shell: ## Open a shell inside the docs image
	$(COMPOSE_DOCS) run --rm --entrypoint sh docs

docs-clean: ## Remove the rendered site directory
	rm -rf ./site

## ----------------------------------------------------------------------------
## WASM video player
## ----------------------------------------------------------------------------

video-player: ## Build the WASM video player
	wasm-pack build --target web ./projects/video_player
