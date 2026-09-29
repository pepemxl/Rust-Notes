PY := python3
VENV := venv
REPONAME=$(basename $(pwd))

.PHONY: build
.PHONY: clean
.PHONY: dev
.PHONY: help
.PHONY: run




help:
	@echo Ayuda
	@echo build - Compila docker
	@echo run - Corre docker image

build:
	docker build -t rust-video-player .

build-dev:
	docker build -t rust-video-player-dev . -f Dockerfile.dev

run:
	docker run -p 8080:8080 --name rust-video-player-container rust-video-player

stop:
	docker stop rust-video-player-container

clean: stop
	docker rm rust-video-player-container

dev2:
	docker run --rm -v "$(pwd)":/usr/src/app -w /usr/src/app -p 8080:8080 rust:latest cargo run --bin video_player

dev:
	docker run --rm -v "$(pwd)":/usr/src/app -w /usr/src/app -p 8080:8080 rust-video-player-dev cargo watch -x 'run --bin video_player'