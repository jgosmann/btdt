ARG BASE_IMAGE=gcr.io/distroless/cc-debian13:nonroot

FROM rust:1 AS builder
ARG TARGETARCH
ARG TARGETABI=gnu
RUN apt-get -y update && apt-get install -y musl-tools && apt-get clean
WORKDIR /app
COPY . /app
RUN case ${TARGETARCH} in \
    arm64) RUSTARCH="aarch64" ;; \
    amd64) RUSTARCH="x86_64" ;; \
    *) RUSTARCH="${TARGETARCH}" ;; \
    esac && \
    rustup target add "${RUSTARCH}-unknown-linux-${TARGETABI}" && \
    cargo build --target "${RUSTARCH}-unknown-linux-${TARGETABI}" --bin btdt-server --release && \
    mv target/"${RUSTARCH}-unknown-linux-${TARGETABI}"/release/btdt-server target/btdt-server
RUN ldd /app/target/release/btdt-server
RUN case ${TARGETARCH} in \
    arm64) DEBARCH="aarch64" ;; \
    amd64) DEBARCH="x86_64" ;; \
    *) DEBARCH="${TARGETARCH}" ;; \
    esac && \
    mkdir -p /tmp/rootfs/lib/${DEBARCH}-linux-gnu && \
    cp /lib/${DEBARCH}-linux-gnu/libzstd.so.1 /tmp/rootfs/lib/${DEBARCH}-linux-gnu/libzstd.so.1

FROM ${BASE_IMAGE}
COPY --from=builder /tmp/rootfs/lib /lib
COPY --from=builder /app/target/btdt-server /btdt-server
ENV BTDT_AUTH_PRIVATE_KEY=/auth_private_key.pem
ENV BTDT_SERVER_CONFIG_FILE=/config.toml
ENV BTDT_TRUSTED_ROOT_CERTS=''
EXPOSE 8707
HEALTHCHECK --start-period=5s --start-interval=1s \
  CMD ["/btdt-server", "health-check", "http://localhost:8707/api/health"]
ENTRYPOINT ["/btdt-server"]
