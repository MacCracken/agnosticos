#!/bin/bash
# download-kernels.sh - Download multiple kernel versions for AGNOS

set -e

KERNEL_BASE_URL="https://cdn.kernel.org/pub/linux/kernel"
KERNEL_SOURCES_DIR="$(dirname "$0")/../sources"

# Kernel versions to download
# Format: version:tag (e.g., "6.6:lts" means 6.6.x LTS)
KERNEL_VERSIONS=(
    "6.6:lts"           # Long-term support
    "6.10:stable"       # Current stable
    "7.0:devel"         # Development placeholder
)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

get_kernel_url() {
    local version=$1
    local major=$(echo "$version" | cut -d. -f1)
    local minor=$(echo "$version" | cut -d. -f2)
    
    # Get latest patch version
    local latest_patch=$(curl -s "${KERNEL_BASE_URL}/v${major}.x/sha256sums.asc" | \
        grep "linux-${major}.${minor}" | \
        grep "\.tar\.xz" | \
        tail -1 | \
        sed -E "s/.*linux-${major}\.${minor}\.([0-9]+)\.tar\.xz.*/\1/")
    
    if [ -z "$latest_patch" ]; then
        latest_patch="0"
    fi
    
    echo "${KERNEL_BASE_URL}/v${major}.x/linux-${major}.${minor}.${latest_patch}.tar.xz"
}

download_kernel() {
    local version=$1
    local tag=$2
    local target_dir="$KERNEL_SOURCES_DIR/$tag"
    local major=$(echo "$version" | cut -d. -f1)
    local minor=$(echo "$version" | cut -d. -f2)
    
    log_info "Downloading Linux kernel $version ($tag)..."
    
    mkdir -p "$target_dir"
    
    if [ "$tag" = "devel" ]; then
        log_warn "Kernel $version is not yet released - creating placeholder"
        cat > "$target_dir/README" << EOF
# Linux $version - Development Placeholder

This directory will contain the Linux $version kernel when it is released.

Expected release: TBD
Current status: Development

AGNOS will support this version once it reaches stable status.
EOF
        return 0
    fi
    
    local kernel_url=$(get_kernel_url "$version")
    local tarball=$(basename "$kernel_url")
    local full_version="${major}.${minor}"
    
    # Check if we can determine the latest patch version
    if curl -sI "$kernel_url" | grep -q "200 OK"; then
        full_version=$(echo "$tarball" | sed 's/linux-//' | sed 's/\.tar\.xz//')
    else
        # Try with .0 if specific version not found
        full_version="${major}.${minor}.0"
        tarball="linux-${full_version}.tar.xz"
        kernel_url="${KERNEL_BASE_URL}/v${major}.x/${tarball}"
    fi
    
    log_info "  -> Downloading ${full_version} from ${kernel_url}"
    
    if [ -f "$target_dir/${tarball}" ]; then
        log_info "  -> Already downloaded: ${tarball}"
    else
        curl -L -o "$target_dir/${tarball}.part" "$kernel_url" || {
            log_error "Failed to download kernel ${full_version}"
            rm -f "$target_dir/${tarball}.part"
            return 1
        }
        mv "$target_dir/${tarball}.part" "$target_dir/${tarball}"
    fi
    
    # Download signature
    if [ ! -f "$target_dir/${tarball}.sign" ]; then
        curl -L -o "$target_dir/${tarball}.sign" "${kernel_url}.sign" || {
            log_warn "Could not download signature"
        }
    fi
    
    # Extract
    if [ -d "$target_dir/linux-${full_version}" ]; then
        log_info "  -> Already extracted"
    else
        log_info "  -> Extracting..."
        tar -xf "$target_dir/${tarball}" -C "$target_dir"
    fi
    
    # Create symlink
    rm -f "$target_dir/linux"
    ln -s "linux-${full_version}" "$target_dir/linux"
    
    log_info "  -> Linux ${full_version} ready in ${target_dir}/"
    
    # Save version info
    echo "${full_version}" > "$target_dir/VERSION"
}

apply_agnos_config() {
    local tag=$1
    local version_file="$KERNEL_SOURCES_DIR/$tag/VERSION"
    
    if [ ! -f "$version_file" ]; then
        log_warn "No version info found for $tag"
        return 1
    fi
    
    local version=$(cat "$version_file")
    local linux_dir="$KERNEL_SOURCES_DIR/$tag/linux"
    local config_target="$KERNEL_SOURCES_DIR/../${tag//:/}/configs/agnos_defconfig"
    
    log_info "Applying AGNOS configuration to $tag ($version)..."
    
    if [ ! -d "$linux_dir" ]; then
        log_error "Linux source not found: $linux_dir"
        return 1
    fi
    
    # Copy AGNOS config
    if [ -f "$config_target" ]; then
        cp "$config_target" "$linux_dir/.config"
        log_info "  -> Config applied"
    else
        log_warn "  -> No AGNOS config found at $config_target"
        log_info "  -> Using default configuration"
        make -C "$linux_dir" defconfig
    fi
    
    # Update config with version-specific settings
    cat >> "$linux_dir/.config" << EOF

# AGNOS Version Specific
CONFIG_LOCALVERSION="-agnos-${tag//:/}"
EOF
    
    # Olddefconfig to resolve any conflicts
    make -C "$linux_dir" olddefconfig
}

verify_kernels() {
    log_info "Verifying kernel downloads..."
    
    for entry in "${KERNEL_VERSIONS[@]}"; do
        local version=$(echo "$entry" | cut -d: -f1)
        local tag=$(echo "$entry" | cut -d: -f2)
        local version_file="$KERNEL_SOURCES_DIR/$tag/VERSION"
        
        if [ -f "$version_file" ]; then
            local actual_version=$(cat "$version_file")
            log_info "  ✓ $tag: Linux $actual_version"
        else
            log_warn "  ✗ $tag: Not downloaded"
        fi
    done
}

show_usage() {
    cat << EOF
Usage: $0 [command] [options]

Commands:
    download    Download all kernel versions
    config      Apply AGNOS configuration to downloaded kernels
    verify      Verify downloaded kernels
    all         Download, configure, and verify all kernels

Options:
    -v, --version SPEC   Download specific version (e.g., "6.6:lts")
    -h, --help          Show this help message

Examples:
    $0 download          # Download all kernels
    $0 config           # Apply AGNOS config to downloaded kernels
    $0 all              # Complete setup
EOF
}

main() {
    case "${1:-all}" in
        download)
            mkdir -p "$KERNEL_SOURCES_DIR"
            for entry in "${KERNEL_VERSIONS[@]}"; do
                local version=$(echo "$entry" | cut -d: -f1)
                local tag=$(echo "$entry" | cut -d: -f2)
                download_kernel "$version" "$tag"
            done
            ;;
        config)
            for entry in "${KERNEL_VERSIONS[@]}"; do
                local tag=$(echo "$entry" | cut -d: -f2)
                apply_agnos_config "$tag"
            done
            ;;
        verify)
            verify_kernels
            ;;
        all)
            mkdir -p "$KERNEL_SOURCES_DIR"
            for entry in "${KERNEL_VERSIONS[@]}"; do
                local version=$(echo "$entry" | cut -d: -f1)
                local tag=$(echo "$entry" | cut -d: -f2)
                download_kernel "$version" "$tag" && \
                apply_agnos_config "$tag"
            done
            verify_kernels
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            log_error "Unknown command: $1"
            show_usage
            exit 1
            ;;
    esac
}

main "$@"
