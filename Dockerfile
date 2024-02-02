FROM rust:latest

RUN apt-get update
RUN apt-get install -y libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstrtspserver-1.0-dev
RUN apt-get install -y cmake
RUN apt-get install -y gstreamer1.0-plugins-bad gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-ugly gstreamer1.0-libav
RUN apt-get install -y libopus-dev
WORKDIR /cordtap
COPY . .
RUN cargo build --release
ENTRYPOINT ["/cordtap/target/release/cordtap"]