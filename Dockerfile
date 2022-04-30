FROM rust as build

WORKDIR /usr/src
RUN USER=root cargo new pastemp
WORKDIR /usr/src/pastemp

# Caches build dependencies by writing placeholder lib and main files.
COPY Cargo.toml Cargo.lock ./

RUN cargo build --release --locked

COPY src ./src
COPY config.toml ./
COPY templates ./templates

# To trigger cargo to recompile
RUN touch src/main.rs

RUN cargo install --path . --offline

FROM debian:buster-slim

RUN apt-get update

COPY --from=build /usr/local/cargo/bin/pastemp /usr/local/bin/pastemp

EXPOSE 8000
CMD ["pastemp"]

