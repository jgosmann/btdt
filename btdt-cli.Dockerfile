FROM rust:1 AS builder
WORKDIR /app
COPY . /app
RUN cargo build --bin btdt --release

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=builder /app/target/release/btdt /btdt
ENTRYPOINT ["/btdt"]
