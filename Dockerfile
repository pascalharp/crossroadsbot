FROM rust:latest

WORKDIR /crossroadsbot
COPY ./Cargo.toml .
COPY ./Cargo.lock .
COPY ./diesel.toml .
COPY ./src ./src
COPY ./migrations ./migrations
RUN cargo install --path .

CMD ["crossroadsbot"]
