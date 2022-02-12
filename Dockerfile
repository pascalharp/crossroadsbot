FROM rust:1.58.1

WORKDIR /crossroadsbot
COPY . .
RUN cargo install --path .

CMD ["crossroadsbot"]
