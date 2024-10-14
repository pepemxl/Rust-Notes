

all:
	make -C ./src/video_player build_video_player


build_video_player:
	wasm-pack build --target web
