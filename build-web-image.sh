#!/usr/bin/env bash
# Build the dhcpdiff-web Docker image and save it as a .tar.gz archive.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

IMAGE="${DHCPDIFF_IMAGE:-dhcpdiff-web}"
TAG="${DHCPDIFF_IMAGE_TAG:-latest}"
OUT="${1:-$ROOT/${IMAGE}.tar.gz}"

FULL_IMAGE="${IMAGE}:${TAG}"

echo "Building ${FULL_IMAGE}…"
docker build -t "$FULL_IMAGE" .

echo "Saving ${FULL_IMAGE} → ${OUT}…"
docker save "$FULL_IMAGE" | gzip -c >"$OUT"

BYTES="$(wc -c <"$OUT" | tr -d ' ')"
echo "Done: ${OUT} (${BYTES} bytes)"
echo "Load later with: docker load < ${OUT}"
