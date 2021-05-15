FROM rust:1.52

WORKDIR /crossroadsbot
COPY . .
RUN cargo install --path .

CMD ["crossroadsbot"]
