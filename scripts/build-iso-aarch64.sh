#!/bin/bash
# build-iso-aarch64.sh — DEPRECATED: use build-sdcard.sh instead
#
# This wrapper exists for backwards compatibility. It calls build-sdcard.sh
# with the appropriate arguments.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[DEPRECATED] build-iso-aarch64.sh is deprecated. Use build-sdcard.sh instead."
echo ""

ARGS=()
for arg in "$@"; do
    if [[ "$arg" == "--edge" ]]; then
        ARGS+=(--profile minimal)
    else
        ARGS+=("$arg")
    fi
done

exec "$SCRIPT_DIR/build-sdcard.sh" "${ARGS[@]}"
