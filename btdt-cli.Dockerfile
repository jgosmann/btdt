ARG BASE_IMAGE=gcr.io/distroless/cc-debian13:nonroot

FROM rust:1 AS builder
WORKDIR /app
COPY . /app
RUN cargo build --bin btdt --release

FROM ${BASE_IMAGE}
COPY --from=builder /app/target/release/btdt /btdt
ENTRYPOINT ["/btdt"]
