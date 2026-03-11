#!/usr/bin/env bash
# ark-install.sh — Install .ark packages into a target root filesystem
#
# Used by container builds and the agnova installer to populate a rootfs
# from pre-built .ark packages without a running AGNOS system.
#
# Usage: ark-install --root /path/to/rootfs --packages /path/to/packages pkg1 pkg2 ...
#
# Each .ark package is a gzipped tarball containing:
#   manifest.toml  — package metadata
#   data.tar.gz    — actual files (rooted at /)
#   files.sha256   — checksums for all installed files

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- Colors ---
if [ -t 1 ]; then
    RED='\033[31m'; GREEN='\033[32m'; YELLOW='\033[33m'; BLUE='\033[36m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BLUE=''; NC=''
fi

log()  { echo -e "${BLUE}[ark-install]${NC} $*"; }
ok()   { echo -e "${GREEN}[ark-install]${NC} $*"; }
warn() { echo -e "${YELLOW}[ark-install]${NC} $*"; }
err()  { echo -e "${RED}[ark-install]${NC} $*" >&2; }
die()  { err "$*"; exit 1; }

# --- Defaults ---
ROOT_DIR=""
PACKAGES_DIR=""
PACKAGES_TO_INSTALL=()
INSTALLED_DB=""
VERBOSE=false

# --- Argument Parsing ---
usage() {
    echo "Usage: ark-install --root DIR --packages DIR [OPTIONS] pkg1 pkg2 ..."
    echo ""
    echo "Options:"
    echo "  --root DIR       Target root filesystem"
    echo "  --packages DIR   Directory containing .ark packages"
    echo "  --verbose        Verbose output"
    echo "  --help           Show this help"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --root)      ROOT_DIR="$2"; shift 2 ;;
            --packages)  PACKAGES_DIR="$2"; shift 2 ;;
            --verbose)   VERBOSE=true; shift ;;
            --help)      usage; exit 0 ;;
            -*)          die "Unknown option: $1" ;;
            *)           PACKAGES_TO_INSTALL+=("$1"); shift ;;
        esac
    done

    [ -n "$ROOT_DIR" ] || die "Missing --root"
    [ -n "$PACKAGES_DIR" ] || die "Missing --packages"
    [ ${#PACKAGES_TO_INSTALL[@]} -gt 0 ] || die "No packages specified"
}

# --- Find .ark file for a package ---
find_ark() {
    local name="$1"
    local ark_file=""

    # Try exact name match first
    for f in "${PACKAGES_DIR}"/*.ark; do
        [ -f "$f" ] || continue
        local basename
        basename=$(basename "$f" .ark)
        # .ark files are named: name-version.ark or name-version-release.ark
        if [[ "$basename" == "${name}"* ]]; then
            ark_file="$f"
            break
        fi
    done

    echo "$ark_file"
}

# --- Install a single .ark package ---
install_ark() {
    local ark_file="$1"
    local pkg_name
    pkg_name=$(basename "$ark_file" .ark)

    log "Installing ${pkg_name}..."

    local tmp_dir
    tmp_dir=$(mktemp -d)

    # Extract the .ark package (outer tarball)
    tar xf "$ark_file" -C "$tmp_dir" 2>/dev/null || {
        # Try as plain gzipped tarball
        tar xzf "$ark_file" -C "$tmp_dir" 2>/dev/null || {
            err "Failed to extract: $ark_file"
            rm -rf "$tmp_dir"
            return 1
        }
    }

    # Look for the data tarball inside
    local data_tar=""
    for candidate in "${tmp_dir}/data.tar.gz" "${tmp_dir}/data.tar.xz" "${tmp_dir}/data.tar"; do
        if [ -f "$candidate" ]; then
            data_tar="$candidate"
            break
        fi
    done

    if [ -n "$data_tar" ]; then
        # Extract data into root
        tar xf "$data_tar" -C "$ROOT_DIR" 2>/dev/null || {
            err "Failed to extract data from: $data_tar"
            rm -rf "$tmp_dir"
            return 1
        }
    else
        # No inner data tarball — the .ark itself contains files directly
        # Strip the manifest and extract everything else
        tar xf "$ark_file" -C "$ROOT_DIR" --exclude='manifest.toml' --exclude='files.sha256' 2>/dev/null || {
            tar xzf "$ark_file" -C "$ROOT_DIR" --exclude='manifest.toml' --exclude='files.sha256' 2>/dev/null || true
        }
    fi

    # Record installation
    if [ -n "$INSTALLED_DB" ]; then
        echo "$pkg_name" >> "$INSTALLED_DB"
    fi

    # Copy manifest to package database
    if [ -f "${tmp_dir}/manifest.toml" ]; then
        local db_dir="${ROOT_DIR}/var/lib/agnos/ark/installed"
        mkdir -p "$db_dir"
        cp "${tmp_dir}/manifest.toml" "${db_dir}/${pkg_name}.toml"
    fi

    rm -rf "$tmp_dir"

    if $VERBOSE; then
        ok "  Installed: $pkg_name"
    fi
}

# --- Main ---
main() {
    parse_args "$@"

    log "Installing ${#PACKAGES_TO_INSTALL[@]} packages into $ROOT_DIR"

    # Setup
    mkdir -p "$ROOT_DIR"
    mkdir -p "${ROOT_DIR}/var/lib/agnos/ark/installed"
    INSTALLED_DB="${ROOT_DIR}/var/lib/agnos/ark/installed.list"
    touch "$INSTALLED_DB"

    local installed=0
    local failed=0
    local skipped=0

    for pkg in "${PACKAGES_TO_INSTALL[@]}"; do
        local ark_file
        ark_file=$(find_ark "$pkg")

        if [ -z "$ark_file" ]; then
            warn "Package not found: $pkg (no .ark in $PACKAGES_DIR)"
            skipped=$((skipped + 1))
            continue
        fi

        if install_ark "$ark_file"; then
            installed=$((installed + 1))
        else
            failed=$((failed + 1))
        fi
    done

    echo ""
    ok "Installation complete: $installed installed, $failed failed, $skipped skipped"

    if [ "$failed" -gt 0 ]; then
        exit 1
    fi
}

main "$@"
