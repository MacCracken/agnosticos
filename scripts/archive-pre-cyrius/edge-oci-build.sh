#!/usr/bin/env bash
# edge-oci-build.sh — Build minimal AGNOS Edge OCI container image
#
# Creates a scratch-based OCI container for edge testing and CI.
# The image contains only the agent-runtime binary (daimon) and
# minimal configuration; it is NOT a full AGNOS Edge OS image.
#
# Usage:
#   ./scripts/edge-oci-build.sh              # builds agnos-edge:latest
#   ./scripts/edge-oci-build.sh my-tag:v1    # builds with custom tag
#
# Prerequisites:
#   - docker or podman
#   - agent-runtime binary built: cargo build --release -p agent-runtime

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TAG="${1:-agnos-edge:latest}"

AGNOS_VERSION="$(cat "$REPO_ROOT/VERSION" 2>/dev/null || echo '2026.3.11')"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC}  $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# ---------------------------------------------------------------------------
# Detect container runtime (docker or podman)
# ---------------------------------------------------------------------------
CONTAINER_RT=""
if command -v docker &>/dev/null; then
    CONTAINER_RT="docker"
elif command -v podman &>/dev/null; then
    CONTAINER_RT="podman"
else
    log_error "Neither docker nor podman found. Install one to build OCI images."
    exit 1
fi
log_info "Using container runtime: $CONTAINER_RT"

# ---------------------------------------------------------------------------
# Locate the agent-runtime binary
# ---------------------------------------------------------------------------
RUNTIME_BIN=""
for candidate in \
    "$REPO_ROOT/target/release/agent_runtime" \
    "$REPO_ROOT/build/release/agent_runtime" \
    "$REPO_ROOT/binaries/agent_runtime"; do
    if [[ -f "$candidate" ]]; then
        RUNTIME_BIN="$candidate"
        break
    fi
done

if [[ -z "$RUNTIME_BIN" ]]; then
    log_error "agent_runtime binary not found."
    log_error "Build it first: cargo build --release -p agent-runtime"
    exit 1
fi

log_info "Using binary: $RUNTIME_BIN"
log_info "Binary size: $(du -h "$RUNTIME_BIN" | cut -f1)"

# ---------------------------------------------------------------------------
# Create temporary build context
# ---------------------------------------------------------------------------
BUILD_CTX="$(mktemp -d)"
trap 'rm -rf "$BUILD_CTX"' EXIT

# Copy the binary into the build context
cp "$RUNTIME_BIN" "$BUILD_CTX/agnos-daimon"
chmod 755 "$BUILD_CTX/agnos-daimon"

# Create minimal /etc files for the container
mkdir -p "$BUILD_CTX/etc/agnos"
echo "$AGNOS_VERSION" > "$BUILD_CTX/etc/agnos/version"
cat > "$BUILD_CTX/etc/agnos/edge.conf" << EOF
# AGNOS Edge OCI Configuration
AGNOS_EDGE_MODE=1
AGNOS_READONLY_ROOTFS=1
AGNOS_VERSION=$AGNOS_VERSION
AGNOS_CONTAINER=oci
EOF

# ---------------------------------------------------------------------------
# Generate Dockerfile
# ---------------------------------------------------------------------------
cat > "$BUILD_CTX/Dockerfile" << 'DOCKERFILE'
# AGNOS Edge — minimal OCI container image
# Built from scratch with only the agent-runtime (daimon) binary.

FROM scratch

# Copy the statically-linked agent-runtime binary
COPY agnos-daimon /usr/bin/agnos-daimon

# Copy minimal configuration
COPY etc/ /etc/

# Create runtime directories (as empty layers)
COPY --from=busybox:musl /bin/true /tmp/.keep

# Environment: edge mode with read-only rootfs
ENV AGNOS_EDGE_MODE=1
ENV AGNOS_READONLY_ROOTFS=1
ENV AGNOS_LOG_FORMAT=json
ENV RUST_LOG=info

# Expose the daimon API port
EXPOSE 8090

# Labels
LABEL org.opencontainers.image.title="AGNOS Edge"
LABEL org.opencontainers.image.description="Minimal AGNOS Edge agent runtime container"
LABEL org.opencontainers.image.source="https://github.com/maccracken/agnosticos"
LABEL org.opencontainers.image.vendor="AGNOS"

ENTRYPOINT ["/usr/bin/agnos-daimon"]
CMD ["daemon"]
DOCKERFILE

# ---------------------------------------------------------------------------
# Build the OCI image
# ---------------------------------------------------------------------------
log_info "Building OCI image: $TAG"
log_info "  Version: $AGNOS_VERSION"

$CONTAINER_RT build \
    --tag "$TAG" \
    --label "org.opencontainers.image.version=$AGNOS_VERSION" \
    --label "org.opencontainers.image.created=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    "$BUILD_CTX"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
IMAGE_SIZE="$($CONTAINER_RT image inspect "$TAG" --format '{{.Size}}' 2>/dev/null || echo 'unknown')"

log_info "============================================"
log_info "  AGNOS Edge OCI Image Built Successfully"
log_info "============================================"
log_info "  Tag:       $TAG"
log_info "  Version:   $AGNOS_VERSION"
log_info "  Size:      $IMAGE_SIZE bytes"
log_info ""
log_info "Run with:"
log_info "  $CONTAINER_RT run -p 8090:8090 $TAG"
log_info ""
log_info "Test health:"
log_info "  curl http://localhost:8090/v1/health"
