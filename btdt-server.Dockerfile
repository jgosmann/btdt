FROM rust:1 AS builder
ARG TARGETARCH
WORKDIR /app
COPY . /app
RUN cargo build --bin btdt-server --release
RUN ldd /app/target/release/btdt-server
RUN case ${TARGETARCH} in \
    arm64) DEBARCH="aarch64" ;; \
    amd64) DEBARCH="x86_64" ;; \
    *) DEBARCH="${TARGETARCH}" ;; \
    esac && \
    mkdir -p /tmp/rootfs/lib/${DEBARCH}-linux-gnu && \
    cp /lib/${DEBARCH}-linux-gnu/libzstd.so.1 /tmp/rootfs/lib/${DEBARCH}-linux-gnu/libzstd.so.1

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=builder /tmp/rootfs/lib /lib
COPY --from=builder /app/target/release/btdt-server /btdt-server
ENV BTDT_AUTH_PRIVATE_KEY=/auth_private_key.pem
ENV BTDT_SERVER_CONFIG_FILE=/config.toml
ENV BTDT_TRUSTED_ROOT_CERTS=''
EXPOSE 8707
HEALTHCHECK --start-period=5s --start-interval=1s \
  CMD ["/btdt-server", "health-check", "http://localhost:8707/api/health"]
ENTRYPOINT ["/btdt-server"]
