FROM rust:1 AS builder
WORKDIR /app
COPY . /app
RUN cargo build --bin btdt-server --release
RUN ldd /app/target/release/btdt-server

FROM gcr.io/distroless/cc-debian13
ARG TARGETARCH
COPY --from=builder /lib/${TARGETARCH/arm64/aarch64}-linux-gnu/libzstd.so.1 /lib/${TARGETARCH/arm64/aarch64}-linux-gnu/libzstd.so.1
COPY --from=builder /app/target/release/btdt-server /btdt-server
ENV BTDT_AUTH_PRIVATE_KEY=/auth_private_key.pem
ENV BTDT_SERVER_CONFIG_FILE=/config.toml
EXPOSE 8707
ENTRYPOINT ["/btdt-server"]
