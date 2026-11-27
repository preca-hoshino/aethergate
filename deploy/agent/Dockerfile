# syntax=docker/dockerfile:1

FROM --platform=$BUILDPLATFORM golang:1.24-bookworm AS singbox-builder
ARG TARGETARCH
ARG SINGBOX_VERSION=v1.13.14
WORKDIR /src
RUN git clone --depth 1 --branch ${SINGBOX_VERSION} https://github.com/SagerNet/sing-box.git sing-box
WORKDIR /src/sing-box
RUN ARCH="${TARGETARCH:-$(dpkg --print-architecture)}" \
    && case "${ARCH}" in \
      amd64) GOARCH=amd64 ;; \
      arm64) GOARCH=arm64 ;; \
      *) echo "Unsupported TARGETARCH: ${ARCH}" >&2; exit 1 ;; \
    esac \
    && CGO_ENABLED=0 GOOS=linux GOARCH="${GOARCH}" go build -v -trimpath \
      -ldflags "-s -w" \
      -tags "with_gvisor,with_quic,with_dhcp,with_wireguard,with_utls,with_acme,with_clash_api,with_v2ray_api,with_tailscale" \
      -o /out/sing-box ./cmd/sing-box

FROM rust:1-bookworm AS rust-builder
ARG TARGETARCH
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends musl-tools binutils ca-certificates \
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

RUN TARGET="$(cat /tmp/rust-target)" \
    && export CC_x86_64_unknown_linux_musl=musl-gcc \
    && export CC_aarch64_unknown_linux_musl=musl-gcc \
    && export RUSTFLAGS="-C target-feature=+crt-static" \
    && cargo build --release --locked -p aethergate-agent --target "${TARGET}" \
    && install -Dm755 "target/${TARGET}/release/aethergate-agent" /out/aethergate-agent \
    && (! readelf -l /out/aethergate-agent | grep -q 'INTERP') \
    && install -d -m 0755 /out/etc/aethergate /out/var/lib/aethergate/wal /out/etc/aethergate/certs

FROM scratch
LABEL org.opencontainers.image.title="Æthergate Agent" \
      org.opencontainers.image.description="Node agent and sing-box supervisor daemon" \
      org.opencontainers.image.source="https://github.com/preca-hoshino/aethergate"

COPY --from=rust-builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=singbox-builder /out/sing-box /usr/local/bin/sing-box
COPY --from=rust-builder /out/aethergate-agent /aethergate-agent
COPY --from=rust-builder /out/etc /etc
COPY --from=rust-builder /out/var /var

ENTRYPOINT ["/aethergate-agent"]
CMD ["--config", "/etc/aethergate/agent.toml"]
