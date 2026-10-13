#!/bin/bash
# build-iso.sh — DEPRECATED: use build-installer.sh instead
#
# This wrapper exists for backwards compatibility. It calls build-installer.sh
# with the appropriate arguments.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[DEPRECATED] build-iso.sh is deprecated. Use build-installer.sh instead."
echo ""

# Map old --edge flag to --profile minimal
ARGS=()
for arg in "$@"; do
    if [[ "$arg" == "--edge" ]]; then
        ARGS+=(--profile minimal)
    else
        ARGS+=("$arg")
    fi
done

exec "$SCRIPT_DIR/build-installer.sh" "${ARGS[@]}"
