# syntax=docker/dockerfile:1

FROM --platform=$BUILDPLATFORM node:22-bookworm-slim AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

FROM rust:1-bookworm AS rust-builder
ARG TARGETARCH
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends musl-tools binutils ca-certificates cmake \
    && rm -rf /var/lib/apt/lists/*

RUN ARCH="${TARGETARCH:-$(dpkg --print-architecture)}" \
    && case "${ARCH}" in \
      amd64) echo "x86_64-unknown-linux-musl" > /tmp/rust-target ;; \
      arm64) echo "aarch64-unknown-linux-musl" > /tmp/rust-target ;; \
      *) echo "Unsupported TARGETARCH: ${ARCH}" >&2; exit 1 ;; \
    esac \
    && rustup target add "$(cat /tmp/rust-target)"

COPY Cargo.toml Cargo.lock sqlx.toml ./
COPY crates/ ./crates/
COPY --from=frontend-builder /app/frontend/dist ./frontend/dist

ARG GIT_COMMIT=unknown
RUN TARGET="$(cat /tmp/rust-target)" \
    && export AETHERGATE_GIT_COMMIT="${GIT_COMMIT}" \
    && export CC_x86_64_unknown_linux_musl=musl-gcc \
    && export CC_aarch64_unknown_linux_musl=musl-gcc \
    && export RUSTFLAGS="-C target-feature=+crt-static" \
    && cargo build --release --locked -p aethergate-server --target "${TARGET}" \
    && install -Dm755 "target/${TARGET}/release/aethergate-server" /out/aethergate-server \
    && (! readelf -l /out/aethergate-server | grep -q 'INTERP') \
    && install -d -m 0755 -o 65532 -g 65532 /out/config

FROM scratch
LABEL org.opencontainers.image.title="Æthergate Server" \
      org.opencontainers.image.description="Control plane for sing-box multi-protocol proxy clusters" \
      org.opencontainers.image.source="https://github.com/preca-hoshino/aethergate"

COPY --from=rust-builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=rust-builder /out/aethergate-server /aethergate-server
COPY --from=rust-builder --chown=65532:65532 /out/config /config

WORKDIR /config
USER 65532:65532
EXPOSE 8080

ENTRYPOINT ["/aethergate-server"]
CMD ["--config", "/config/config.server.toml"]
