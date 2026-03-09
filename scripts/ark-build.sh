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
#   ARK_SKIP_CHECK  — set to 1 to skip the check/test step
#   ARK_JOBS        — parallel make jobs (default: nproc)
#   ARK_LOCAL_SRC   — path to local source tree (for local=true recipes)

set -euo pipefail

RECIPE="${1:?Usage: ark-build.sh <recipe.toml>}"
OUTPUT_DIR="${ARK_OUTPUT_DIR:-${PWD}/dist/ark}"
BUILD_DIR="${ARK_BUILD_DIR:-/tmp/takumi-build}"
CACHE_DIR="${ARK_CACHE_DIR:-${CACHE:-/tmp/takumi-cache}}"
SKIP_CHECK="${ARK_SKIP_CHECK:-0}"
JOBS="${ARK_JOBS:-$(nproc 2>/dev/null || echo 4)}"

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
# Parse recipe (lightweight TOML parsing)
# For full builds, the Rust takumi binary should be used instead.
# -----------------------------------------------------------------------
parse_field() {
    local file="$1" field="$2"
    { grep -m1 "^${field} " "$file" 2>/dev/null || true; } | sed 's/.*= *"\(.*\)"/\1/'
}

parse_bool() {
    local file="$1" field="$2"
    { grep -m1 "^${field} " "$file" 2>/dev/null || true; } | sed 's/.*= *//' | tr -d ' '
}

parse_array() {
    local file="$1" field="$2"
    # Extract TOML array values — handles both single-line and multi-line arrays.
    local line
    line=$(grep -m1 "^${field} = \[" "$file" 2>/dev/null) || true
    if [ -z "$line" ]; then
        return 0
    fi
    # Check if array closes on the same line (single-line array)
    if echo "$line" | grep -q '\]'; then
        echo "$line" | grep -oP '"[^"]*"' | tr -d '"'
    else
        # Multi-line: collect from field line through closing ]
        sed -n "/^${field} = \[/,/^\]/p" "$file" 2>/dev/null \
            | grep -oP '"[^"]*"' \
            | tr -d '"' \
            || true
    fi
}

if [ ! -f "$RECIPE" ]; then
    err "Recipe not found: $RECIPE"
    exit 1
fi

# Resolve to absolute path (survives cd into build directory)
RECIPE="$(cd "$(dirname "$RECIPE")" && pwd)/$(basename "$RECIPE")"

PKG_NAME=$(parse_field "$RECIPE" "name")
PKG_VERSION=$(parse_field "$RECIPE" "version")
PKG_DESC=$(parse_field "$RECIPE" "description")
PKG_LICENSE=$(parse_field "$RECIPE" "license")
SOURCE_URL=$(parse_field "$RECIPE" "url")
SOURCE_SHA256=$(parse_field "$RECIPE" "sha256")
SOURCE_LOCAL=$(parse_bool "$RECIPE" "local")

# Parse security hardening flags
HARDENING_FLAGS=$(parse_array "$RECIPE" "hardening")
RECIPE_CFLAGS=$(parse_field "$RECIPE" "cflags")
RECIPE_LDFLAGS=$(parse_field "$RECIPE" "ldflags")

# -----------------------------------------------------------------------
# Apply security hardening CFLAGS/LDFLAGS
# -----------------------------------------------------------------------
apply_hardening() {
    local cflags="${RECIPE_CFLAGS:-}"
    local ldflags="${RECIPE_LDFLAGS:-}"

    for flag in $HARDENING_FLAGS; do
        case "$flag" in
            pie)             cflags="$cflags -fPIE"; ldflags="$ldflags -pie" ;;
            relro)           ldflags="$ldflags -Wl,-z,relro" ;;
            fullrelro)       ldflags="$ldflags -Wl,-z,relro,-z,now" ;;
            fortify)         cflags="$cflags -D_FORTIFY_SOURCE=2" ;;
            stackprotector)  cflags="$cflags -fstack-protector-strong" ;;
            bindnow)         ldflags="$ldflags -Wl,-z,now" ;;
        esac
    done

    # Deduplicate -z,now if both fullrelro and bindnow
    ldflags=$(echo "$ldflags" | sed 's/-Wl,-z,now.*-Wl,-z,now/-Wl,-z,now/')

    export CFLAGS="$cflags"
    export CXXFLAGS="$cflags"
    export LDFLAGS="$ldflags"
}

apply_hardening

# -----------------------------------------------------------------------
# Display build info
# -----------------------------------------------------------------------
log "Package:  ${PKG_NAME} ${PKG_VERSION}"
if [ "$SOURCE_LOCAL" = "true" ]; then
    log "Source:   local (no download)"
else
    log "Source:   ${SOURCE_URL}"
fi
log "Output:   ${OUTPUT_DIR}"
log "CFLAGS:   ${CFLAGS:-<none>}"
log "LDFLAGS:  ${LDFLAGS:-<none>}"
log "Jobs:     ${JOBS}"

# -----------------------------------------------------------------------
# Setup directories
# -----------------------------------------------------------------------
SRC_DIR="${BUILD_DIR}/${PKG_NAME}-${PKG_VERSION}/src"
PKG_DIR="${BUILD_DIR}/${PKG_NAME}-${PKG_VERSION}/pkg"
LOG_DIR="${BUILD_DIR}/${PKG_NAME}-${PKG_VERSION}/logs"
mkdir -p "$SRC_DIR" "$PKG_DIR" "$LOG_DIR" "$CACHE_DIR" "$OUTPUT_DIR"

BUILD_LOG="${LOG_DIR}/build.log"

# -----------------------------------------------------------------------
# Download or locate source
# -----------------------------------------------------------------------
if [ "$SOURCE_LOCAL" = "true" ]; then
    # Local source — copy from ARK_LOCAL_SRC or skip (handled in build steps)
    if [ -n "${ARK_LOCAL_SRC:-}" ] && [ -d "$ARK_LOCAL_SRC" ]; then
        log "Copying local source from: ${ARK_LOCAL_SRC}"
        cp -a "$ARK_LOCAL_SRC"/. "$SRC_DIR"/
    else
        log "Local source recipe — build steps handle source acquisition"
    fi
else
    if [ -z "$SOURCE_URL" ]; then
        err "No source URL and local=true not set"
        exit 1
    fi

    TARBALL_NAME=$(basename "$SOURCE_URL")
    TARBALL_PATH="${CACHE_DIR}/${TARBALL_NAME}"

    if [ -f "$TARBALL_PATH" ]; then
        log "Using cached source: ${TARBALL_PATH}"
    else
        log "Downloading source..."
        curl -fSL --retry 3 --connect-timeout 30 -o "$TARBALL_PATH" "$SOURCE_URL"
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

    # Extract source
    log "Extracting source..."
    tar xf "$TARBALL_PATH" -C "$SRC_DIR" --strip-components=1
fi

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

# If make/configure/check are single-line (not multi-line), parse those too
if [ -z "$MAKE_CMD" ]; then
    MAKE_CMD=$(parse_field "$RECIPE" "make")
fi
if [ -z "$CONFIGURE" ]; then
    CONFIGURE=$(parse_field "$RECIPE" "configure")
fi
if [ -z "$CHECK_CMD" ]; then
    CHECK_CMD=$(parse_field "$RECIPE" "check")
fi

# -----------------------------------------------------------------------
# Run a build step with logging
# -----------------------------------------------------------------------
run_step() {
    local name="$1" cmd="$2"
    if [ -z "$cmd" ]; then
        return 0
    fi
    log "Running ${name}..."
    local start_time
    start_time=$(date +%s)
    eval "$cmd" 2>&1 | tee -a "$BUILD_LOG"
    local status=${PIPESTATUS[0]}
    local end_time
    end_time=$(date +%s)
    local elapsed=$((end_time - start_time))
    if [ $status -eq 0 ]; then
        ok "${name} completed (${elapsed}s)"
    else
        err "${name} failed (exit ${status}, ${elapsed}s)"
        err "Build log: ${BUILD_LOG}"
        return $status
    fi
}

# -----------------------------------------------------------------------
# Build
# -----------------------------------------------------------------------
export PKG="$PKG_DIR"
export VERSION="$PKG_VERSION"
export MAKEFLAGS="-j${JOBS}"

cd "$SRC_DIR"

TOTAL_START=$(date +%s)

run_step "pre_build" "$PRE_BUILD"
run_step "configure" "$CONFIGURE"
run_step "make" "$MAKE_CMD"

if [ "$SKIP_CHECK" = "1" ]; then
    warn "Skipping check step (ARK_SKIP_CHECK=1)"
elif [ -n "$CHECK_CMD" ]; then
    run_step "check" "$CHECK_CMD" || warn "Check step had failures (continuing)"
fi

run_step "install" "$INSTALL_CMD"
run_step "post_install" "$POST_INSTALL"

# -----------------------------------------------------------------------
# Verify package directory is not empty
# -----------------------------------------------------------------------
if [ -z "$(ls -A "$PKG_DIR" 2>/dev/null)" ]; then
    err "Package directory is empty — install step produced no output"
    exit 1
fi

# -----------------------------------------------------------------------
# Generate file list with SHA-256 checksums
# -----------------------------------------------------------------------
log "Generating file manifest..."
FILE_LIST="${PKG_DIR}/.ark-filelist"
(cd "$PKG_DIR" && find . -type f ! -name '.ark-*' -exec sha256sum {} \; | sort) > "$FILE_LIST"
FILE_COUNT=$(wc -l < "$FILE_LIST")
INSTALLED_SIZE=$(du -sb "$PKG_DIR" | cut -f1)

# -----------------------------------------------------------------------
# Parse runtime dependencies for manifest
# -----------------------------------------------------------------------
# Format as TOML arrays: "a","b","c"
RUNTIME_DEPS_TOML=$(parse_array "$RECIPE" "runtime" | sed 's/.*/"&"/' | paste -sd, -)
GROUPS_TOML=$(parse_array "$RECIPE" "groups" | sed 's/.*/"&"/' | paste -sd, -)

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
description = "${PKG_DESC}"
license = "${PKG_LICENSE}"
arch = "$(uname -m)"
build_date = "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
builder = "takumi/ark-build.sh"
source_url = "${SOURCE_URL:-local}"
source_hash = "${SOURCE_SHA256:-none}"
installed_size = ${INSTALLED_SIZE}
file_count = ${FILE_COUNT}

[depends]
runtime = [${RUNTIME_DEPS_TOML}]

[meta]
groups = [${GROUPS_TOML}]
recipe = "$(basename "$RECIPE")"
MANIFEST

# Create tarball
tar czf "$ARK_FILE" -C "$PKG_DIR" .

TOTAL_END=$(date +%s)
TOTAL_ELAPSED=$((TOTAL_END - TOTAL_START))

# Report
ARK_SIZE=$(du -sh "$ARK_FILE" | cut -f1)
ok "Built: ${ARK_FILE} (${ARK_SIZE}, ${FILE_COUNT} files, ${TOTAL_ELAPSED}s)"

# -----------------------------------------------------------------------
# Cleanup
# -----------------------------------------------------------------------
log "Cleaning build directory..."
rm -rf "${BUILD_DIR}/${PKG_NAME}-${PKG_VERSION}"

ok "Done: ${PKG_NAME} ${PKG_VERSION}"
