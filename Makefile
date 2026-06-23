# Rust Notes — project Makefile
#
# Run `make` or `make help` to list available targets.

COMPOSE_DOCS := docker compose -f docker-compose.docs.yml

.DEFAULT_GOAL := help

.PHONY: help all \
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
## MkDocs documentation
## ----------------------------------------------------------------------------

docs-serve: ## Serve docs with live reload at http://localhost:8000
	$(COMPOSE_DOCS) up --build docs

docs-build: ## Render the static site into ./site
	$(COMPOSE_DOCS) --profile build run --rm build

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
	wasm-pack build --target web ./src/video_player
