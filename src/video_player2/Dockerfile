# FROM rust:latest AS builder

# WORKDIR /usr/src/video_player

# COPY . .

# RUN apt-get update \
#     && DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y \
#     wget \
#     g++ \
#     build-essential \
#     libssl-dev \
#     pkg-config \
#     libavutil-dev \
#     libavformat-dev \
#     libavfilter-dev \ 
#     llvm \ 
#     libclang-dev \
#     libgcc-7-dev \
#     ffmpeg

# RUN cargo install --path .

#FROM debian:buster-slim
FROM ubuntu:24.04

# RUN apt-get update \
#     && DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y \
#     wget \
#     g++ \
#     build-essential \
#     libssl-dev \
#     pkg-config \
#     libavutil-dev \
#     libavformat-dev \
#     libavfilter-dev \ 
#     llvm \ 
#     libclang-dev \
#     ffmpeg

    
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    wget \
    g++ \
    build-essential \
#    ca-certificates \
    clang \
    curl \
    ffmpeg \
    git \
    libavcodec-dev \
    libavdevice-dev \
    libavfilter-dev \
    libavformat-dev \
#    libavresample-dev \
    libavutil-dev \
    libpostproc-dev \
#    libswresample-dev \
    libswscale-dev \
    pkg-config \
    vim 
    
RUN curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh

ENV PATH=/root/.cargo/bin:$PATH


#RUN git clone https://github.com/zmwangx/rust-ffmpeg-sys /ffmpeg-sys && \
#    cd /ffmpeg-sys && \
#    cargo build

WORKDIR /usr/src/video_player

RUN cargo install --path .

# COPY --from=builder /usr/local/cargo/bin/video_player /usr/local/bin/video_player
#COPY src/index.html src/index.html
#COPY ./static/ /static/

#CMD ["video_player"]

#EXPOSE 8080