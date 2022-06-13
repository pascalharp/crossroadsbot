FROM rust:latest

WORKDIR /crossroadsbot
COPY . .
RUN cargo install --path .

CMD ["crossroadsbot"]
