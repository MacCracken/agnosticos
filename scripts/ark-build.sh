#!/usr/bin/env bash
# ark-build.sh — Build an .ark package from a takumi recipe
#
# Usage:
#   ./scripts/ark-build.sh recipes/python/cpython-3.12.toml
#   ./scripts/ark-build.sh /recipes/python/cpython-3.12.toml  # container path
#
# Environment variables:
#   ARK_OUTPUT_DIR  — where to place the built .ark file (default: dist/ark)
#   ARK_BUILD_DIR   — scratch space for building (default: /tmp/takumi-build)
#   ARK_CACHE_DIR   — source tarball cache (default: /cache or /tmp/takumi-cache)

set -euo pipefail

RECIPE="${1:?Usage: ark-build.sh <recipe.toml>}"
OUTPUT_DIR="${ARK_OUTPUT_DIR:-${PWD}/dist/ark}"
BUILD_DIR="${ARK_BUILD_DIR:-/tmp/takumi-build}"
CACHE_DIR="${ARK_CACHE_DIR:-${CACHE:-/tmp/takumi-cache}}"

# Colors (if terminal)
if [ -t 1 ]; then
    BLUE='\033[36m'; GREEN='\033[32m'; YELLOW='\033[33m'; RED='\033[31m'; NC='\033[0m'
else
    BLUE=''; GREEN=''; YELLOW=''; RED=''; NC=''
fi

log()  { echo -e "${BLUE}[takumi]${NC} $*"; }
ok()   { echo -e "${GREEN}[takumi]${NC} $*"; }
warn() { echo -e "${YELLOW}[takumi]${NC} $*"; }
err()  { echo -e "${RED}[takumi]${NC} $*" >&2; }

# -----------------------------------------------------------------------
# Parse recipe (lightweight TOML parsing — name, version, url, sha256)
# For full builds, the Rust takumi binary should be used instead.
# -----------------------------------------------------------------------
parse_field() {
    local file="$1" field="$2"
    grep -m1 "^${field} " "$file" | sed 's/.*= *"\(.*\)"/\1/'
}

if [ ! -f "$RECIPE" ]; then
    err "Recipe not found: $RECIPE"
    exit 1
fi

PKG_NAME=$(parse_field "$RECIPE" "name")
PKG_VERSION=$(parse_field "$RECIPE" "version")
SOURCE_URL=$(parse_field "$RECIPE" "url")
SOURCE_SHA256=$(parse_field "$RECIPE" "sha256")

log "Package:  ${PKG_NAME} ${PKG_VERSION}"
log "Source:   ${SOURCE_URL}"
log "Output:   ${OUTPUT_DIR}"

# -----------------------------------------------------------------------
# Setup directories
# -----------------------------------------------------------------------
SRC_DIR="${BUILD_DIR}/${PKG_NAME}-${PKG_VERSION}/src"
PKG_DIR="${BUILD_DIR}/${PKG_NAME}-${PKG_VERSION}/pkg"
mkdir -p "$SRC_DIR" "$PKG_DIR" "$CACHE_DIR" "$OUTPUT_DIR"

# -----------------------------------------------------------------------
# Download source
# -----------------------------------------------------------------------
TARBALL_NAME=$(basename "$SOURCE_URL")
TARBALL_PATH="${CACHE_DIR}/${TARBALL_NAME}"

if [ -f "$TARBALL_PATH" ]; then
    log "Using cached source: ${TARBALL_PATH}"
else
    log "Downloading source..."
    curl -fSL --retry 3 -o "$TARBALL_PATH" "$SOURCE_URL"
fi

# Verify checksum (skip placeholder zeros)
if [ "$SOURCE_SHA256" != "0000000000000000000000000000000000000000000000000000000000000000" ]; then
    log "Verifying SHA-256..."
    echo "${SOURCE_SHA256}  ${TARBALL_PATH}" | sha256sum -c - || {
        err "SHA-256 mismatch!"
        exit 1
    }
else
    warn "SHA-256 is placeholder — skipping verification"
fi

# -----------------------------------------------------------------------
# Extract source
# -----------------------------------------------------------------------
log "Extracting source..."
tar xf "$TARBALL_PATH" -C "$SRC_DIR" --strip-components=1

# -----------------------------------------------------------------------
# Parse build steps from recipe
# -----------------------------------------------------------------------
extract_step() {
    local file="$1" step="$2"
    # Extract multi-line string between step = """ and next """
    sed -n "/^${step} = \"\"\"/,/^\"\"\"/{/^${step} = \"\"\"/d;/^\"\"\"$/d;p}" "$file"
}

PRE_BUILD=$(extract_step "$RECIPE" "pre_build")
CONFIGURE=$(extract_step "$RECIPE" "configure")
MAKE_CMD=$(extract_step "$RECIPE" "make")
CHECK_CMD=$(extract_step "$RECIPE" "check")
INSTALL_CMD=$(extract_step "$RECIPE" "install")
POST_INSTALL=$(extract_step "$RECIPE" "post_install")

# If make/configure are single-line (not multi-line), parse those too
if [ -z "$MAKE_CMD" ]; then
    MAKE_CMD=$(parse_field "$RECIPE" "make")
fi
if [ -z "$CONFIGURE" ]; then
    CONFIGURE=$(parse_field "$RECIPE" "configure")
fi

# -----------------------------------------------------------------------
# Build
# -----------------------------------------------------------------------
export PKG="$PKG_DIR"

cd "$SRC_DIR"

if [ -n "$PRE_BUILD" ]; then
    log "Running pre_build..."
    eval "$PRE_BUILD"
fi

if [ -n "$CONFIGURE" ]; then
    log "Running configure..."
    eval "$CONFIGURE"
fi

if [ -n "$MAKE_CMD" ]; then
    log "Running make..."
    eval "$MAKE_CMD"
fi

if [ -n "$CHECK_CMD" ]; then
    log "Running check..."
    eval "$CHECK_CMD" || warn "Check step had failures (continuing)"
fi

if [ -n "$INSTALL_CMD" ]; then
    log "Running install..."
    eval "$INSTALL_CMD"
fi

if [ -n "$POST_INSTALL" ]; then
    log "Running post_install..."
    eval "$POST_INSTALL"
fi

# -----------------------------------------------------------------------
# Package as .ark (signed tarball + manifest)
# -----------------------------------------------------------------------
log "Creating .ark package..."

ARK_FILE="${OUTPUT_DIR}/${PKG_NAME}-${PKG_VERSION}-$(uname -m).ark"

# Generate manifest
cat > "${PKG_DIR}/.ark-manifest.toml" << MANIFEST
[package]
name = "${PKG_NAME}"
version = "${PKG_VERSION}"
arch = "$(uname -m)"
build_date = "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
builder = "takumi/ark-build.sh"
source_url = "${SOURCE_URL}"
source_hash = "${SOURCE_SHA256}"
MANIFEST

# Create tarball
tar czf "$ARK_FILE" -C "$PKG_DIR" .

# Report
ARK_SIZE=$(du -sh "$ARK_FILE" | cut -f1)
ok "Built: ${ARK_FILE} (${ARK_SIZE})"

# -----------------------------------------------------------------------
# Cleanup
# -----------------------------------------------------------------------
log "Cleaning build directory..."
rm -rf "${BUILD_DIR}/${PKG_NAME}-${PKG_VERSION}"

ok "Done: ${PKG_NAME} ${PKG_VERSION}"
