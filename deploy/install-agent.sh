#!/usr/bin/env bash
set -euo pipefail

# Installs aethergate-agent + systemd service on Linux (amd64/arm64).
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) TARGET="x86_64-unknown-linux-musl" ;;
  aarch64|arm64) TARGET="aarch64-unknown-linux-musl" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

REPO="${AETHERGATE_REPO:-aethergate/aethergate}"
VERSION="${AETHERGATE_VERSION:-latest}"

mkdir -p /etc/aethergate/certs /var/lib/aethergate/wal

if [ "$VERSION" = "latest" ]; then
  URL="https://github.com/${REPO}/releases/latest/download/aethergate-agent-${TARGET}"
else
  URL="https://github.com/${REPO}/releases/download/${VERSION}/aethergate-agent-${TARGET}"
fi

curl -fsSL "$URL" -o /usr/local/bin/aethergate-agent
chmod +x /usr/local/bin/aethergate-agent

curl -fsSL "https://raw.githubusercontent.com/${REPO}/main/deploy/systemd/aethergate-agent.service" \
  -o /etc/systemd/system/aethergate-agent.service

systemctl daemon-reload
systemctl enable aethergate-agent
echo "Installed aethergate-agent. Edit /etc/aethergate/config.agent.toml then run: systemctl start aethergate-agent"
