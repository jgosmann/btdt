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
    cargo build --target "${RUSTARCH}-unknown-linux-${TARGETABI}" --bin btdt --release && \
    mv target/"${RUSTARCH}-unknown-linux-${TARGETABI}"/release/btdt target/btdt

FROM ${BASE_IMAGE}
COPY --from=builder /app/target/btdt /btdt
ENTRYPOINT ["/btdt"]
