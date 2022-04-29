FROM rust as build

WORKDIR /usr/src
RUN USER=root cargo new pastemp
WORKDIR /usr/src/pastemp

# Caches build dependencies by writing placeholder lib and main files.
COPY Cargo.toml Cargo.lock ./

COPY src ./src
COPY config.toml ./

RUN cargo install --path .

FROM debian:buster-slim

RUN apt-get update

COPY --from=build /usr/local/cargo/bin/pastemp /usr/local/bin/pastemp

EXPOSE 8000
CMD ["pastemp"]

