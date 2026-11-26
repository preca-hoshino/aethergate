#!/usr/bin/env bash
set -euo pipefail

IMAGE="${1:-aethergate-agent:test}"
echo "Verifying scratch agent image: $IMAGE"
docker run --rm --entrypoint /aethergate-agent "$IMAGE" --version
docker run --rm --entrypoint /usr/local/bin/sing-box "$IMAGE" version
echo "Agent container verification passed."
