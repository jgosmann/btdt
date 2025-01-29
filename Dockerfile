FROM rust:1 AS builder
WORKDIR /usr/src/btdt
COPY . .
RUN cargo install --path .

FROM debian:bookworm-slim
COPY --from=builder /usr/local/cargo/bin/btdt /usr/local/bin/btdt
CMD ["btdt"]
