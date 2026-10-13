#!/bin/bash
# build-kernel.sh - Build AGNOS kernel for specified version

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KERNEL_DIR="$SCRIPT_DIR/../kernel"
BUILD_DIR="$KERNEL_DIR/build"
OUTPUT_DIR="$SCRIPT_DIR/../output"

# Default values
KERNEL_VERSION="6.6-lts"
MAKE_JOBS=$(nproc)
CLEAN_BUILD=false
VERBOSE=false

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

usage() {
    cat << EOF
Usage: $0 [options]

Build AGNOS kernel for a specific version

Options:
    -v, --version VERSION   Kernel version to build (6.6-lts, 6.x-stable, 7.0-devel)
    -j, --jobs N            Number of parallel jobs (default: $(nproc))
    -c, --clean             Clean build (remove previous build)
    -V, --verbose           Verbose output
    -h, --help              Show this help message

Examples:
    $0 -v 6.6-lts          # Build 6.6 LTS kernel
    $0 -v 6.x-stable -c    # Clean build of current stable
    $0 -j 8                # Build with 8 parallel jobs
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -v|--version)
                KERNEL_VERSION="$2"
                shift 2
                ;;
            -j|--jobs)
                MAKE_JOBS="$2"
                shift 2
                ;;
            -c|--clean)
                CLEAN_BUILD=true
                shift
                ;;
            -V|--verbose)
                VERBOSE=true
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done
}

setup_build_env() {
    log_info "Setting up build environment..."
    
    local version_dir="$KERNEL_DIR/$KERNEL_VERSION"
    local source_dir="$KERNEL_DIR/sources/$(echo $KERNEL_VERSION | tr ':' '-')"
    
    if [ ! -d "$version_dir" ]; then
        log_error "Kernel version not found: $KERNEL_VERSION"
        log_info "Available versions:"
        ls -1 "$KERNEL_DIR" | grep -E '^[0-9]'
        exit 1
    fi
    
    # Create build directory
    local build_target="$BUILD_DIR/$KERNEL_VERSION"
    
    if [ "$CLEAN_BUILD" = true ] && [ -d "$build_target" ]; then
        log_info "Cleaning previous build..."
        rm -rf "$build_target"
    fi
    
    mkdir -p "$build_target"
    mkdir -p "$OUTPUT_DIR"
    
    echo "$build_target"
}

copy_source() {
    local build_target="$1"
    local source_dir="$KERNEL_DIR/sources/$(echo $KERNEL_VERSION | tr ':' '-')"
    
    log_info "Copying kernel source..."
    
    if [ ! -d "$source_dir/linux" ]; then
        log_error "Kernel source not found. Run download-kernels.sh first."
        exit 1
    fi
    
    # Use rsync if available, otherwise cp -r
    if command -v rsync &> /dev/null; then
        rsync -a --exclude='.git' "$source_dir/linux/" "$build_target/"
    else
        cp -r "$source_dir/linux/"* "$build_target/"
    fi
}

apply_patches() {
    local build_target="$1"
    local patches_dir="$KERNEL_DIR/$KERNEL_VERSION/patches"
    
    log_info "Applying AGNOS patches..."
    
    if [ -d "$patches_dir" ]; then
        for patch_dir in "$patches_dir"/*; do
            if [ -d "$patch_dir" ]; then
                log_info "  -> Applying patches from $(basename "$patch_dir")..."
                for patch in "$patch_dir"/*.patch; do
                    if [ -f "$patch" ]; then
                        if patch -p1 --dry-run -i "$patch" > /dev/null 2>&1; then
                            patch -p1 -i "$patch"
                            log_info "     Applied: $(basename "$patch")"
                        else
                            log_warn "     Skipped: $(basename "$patch") (already applied or incompatible)"
                        fi
                    fi
                done
            fi
        done
    fi
}

configure_kernel() {
    local build_target="$1"
    local config_file="$KERNEL_DIR/$KERNEL_VERSION/configs/agnos_defconfig"
    
    log_info "Configuring kernel..."
    
    cd "$build_target"
    
    if [ -f "$config_file" ]; then
        cp "$config_file" .config
        log_info "  -> Using AGNOS configuration"
    else
        make defconfig
        log_info "  -> Using default configuration"
    fi
    
    # Apply any additional config fragments
    for fragment in "$KERNEL_DIR/$KERNEL_VERSION/configs/"*.config; do
        if [ -f "$fragment" ] && [ "$fragment" != "$config_file" ]; then
            log_info "  -> Merging config: $(basename "$fragment")"
            ./scripts/kconfig/merge_config.sh -m .config "$fragment"
        fi
    done
    
    # Update configuration
    make olddefconfig
    
    if [ "$VERBOSE" = true ]; then
        log_info "  -> Kernel configuration:"
        grep "^CONFIG_LOCALVERSION" .config
        grep "^CONFIG_DEFAULT_HOSTNAME" .config || true
    fi
}

build_kernel() {
    local build_target="$1"
    
    log_info "Building kernel (this may take a while)..."
    
    cd "$build_target"
    
    local make_opts="-j$MAKE_JOBS"
    if [ "$VERBOSE" = true ]; then
        make_opts="V=1 $make_opts"
    else
        make_opts="V=0 $make_opts"
    fi
    
    # Build kernel
    make $make_opts bzImage
    
    # Build modules
    make $make_opts modules
    
    log_info "Kernel build complete"
}

package_kernel() {
    local build_target="$1"
    local version_name=$(echo "$KERNEL_VERSION" | tr ':' '-')
    local package_dir="$OUTPUT_DIR/kernel-$version_name"
    
    log_info "Packaging kernel..."
    
    rm -rf "$package_dir"
    mkdir -p "$package_dir/boot"
    mkdir -p "$package_dir/lib/modules"
    
    cd "$build_target"
    
    # Copy kernel image
    cp "arch/x86/boot/bzImage" "$package_dir/boot/vmlinuz-agnos-$version_name"
    
    # Copy System.map and config
    cp System.map "$package_dir/boot/System.map-agnos-$version_name"
    cp .config "$package_dir/boot/config-agnos-$version_name"
    
    # Install modules
    make INSTALL_MOD_PATH="$package_dir" modules_install
    
    # Create module dependencies
    depmod -b "$package_dir" $(make kernelrelease)
    
    # Create tarball
    local tarball="$OUTPUT_DIR/agnos-kernel-$version_name.tar.xz"
    log_info "  -> Creating $tarball"
    tar -cJf "$tarball" -C "$package_dir" .
    
    # Calculate checksums
    sha256sum "$tarball" > "$tarball.sha256"
    
    log_info "Kernel package created: $tarball"
}

build_agnos_modules() {
    local build_target="$1"
    
    log_info "Building AGNOS kernel modules..."
    
    local modules_source="$KERNEL_DIR/modules"
    
    if [ ! -d "$modules_source" ]; then
        log_warn "AGNOS modules source not found, skipping"
        return
    fi
    
    cd "$build_target"
    
    # Build each module
    for module_dir in "$modules_source"/*; do
        if [ -d "$module_dir" ] && [ -f "$module_dir/Kbuild" ]; then
            local module_name=$(basename "$module_dir")
            log_info "  -> Building $module_name..."
            
            make M="$module_dir" $make_opts modules
        fi
    done
}

main() {
    parse_args "$@"
    
    log_info "Building AGNOS kernel: $KERNEL_VERSION"
    log_info "  Build jobs: $MAKE_JOBS"
    log_info "  Clean build: $CLEAN_BUILD"
    
    local build_target=$(setup_build_env)
    copy_source "$build_target"
    apply_patches "$build_target"
    configure_kernel "$build_target"
    build_kernel "$build_target"
    build_agnos_modules "$build_target"
    package_kernel "$build_target"
    
    log_info "Build complete!"
    log_info "Output: $OUTPUT_DIR"
}

main "$@"
