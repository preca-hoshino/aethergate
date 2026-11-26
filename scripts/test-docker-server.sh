#!/usr/bin/env bash
set -euo pipefail

IMAGE="${1:-aethergate-server:test}"
echo "Verifying scratch server image: $IMAGE"
docker run --rm --entrypoint /aethergate-server "$IMAGE" --version
docker run --rm --entrypoint /aethergate-server "$IMAGE" --help >/dev/null
echo "Server container verification passed."
