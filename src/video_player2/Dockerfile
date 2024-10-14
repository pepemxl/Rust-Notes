FROM rust:latest AS builder

WORKDIR /usr/src/video_player

COPY . .

RUN cargo install --path .

#FROM debian:buster-slim
FROM ubuntu:24.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y \
    wget \
    g++ \
    build-essential \
    libssl-dev \
    pkg-config \
    libavutil-dev \
    libavformat-dev \
    libavfilter-dev \ 
    llvm \ 
    libclang-dev \
    ffmpeg

COPY --from=builder /usr/local/cargo/bin/video_player /usr/local/bin/video_player
COPY src/index.html src/index.html
COPY ./static/video.mp4 /static/video.mp4

CMD ["video_player"]

EXPOSE 8080