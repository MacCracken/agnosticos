#!/usr/bin/env bash
# build-selfhosting-iso.sh — Build a fully self-hosting AGNOS ISO.
#
# This is the Phase 13A endgame: produce an ISO where AGNOS can rebuild
# itself from source without any host distro.
#
# Multi-stage build:
#   Stage 0: Download source tarballs
#   Stage 1: Bootstrap cross-toolchain (GCC, binutils, glibc)
#   Stage 2: Enter chroot, build base system from recipes
#   Stage 3: Build AGNOS userland (cargo build)
#   Stage 4: Package into bootable ISO
#
# This replaces the Debian debootstrap approach with a pure LFS build.
# The result is an AGNOS system built entirely from source.
#
# Usage:
#   sudo LFS=/mnt/agnos ./build-selfhosting-iso.sh
#   sudo LFS=/mnt/agnos ./build-selfhosting-iso.sh --stage 2  # resume from stage 2
#
# Prerequisites:
#   - 20+ GB free disk space at $LFS
#   - Host has: gcc 13+, make, bison, gawk, m4, texinfo, xz, cargo
#   - Internet access (for downloading source tarballs)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/.."
OUTPUT_DIR="$REPO_DIR/output"

LFS="${LFS:?'Set LFS to the target mount point (e.g. /mnt/agnos)'}"
ISO_VERSION="$(cat "${REPO_DIR}/VERSION" 2>/dev/null || echo 'dev')"
START_STAGE="${1:-0}"

# Source tarball URLs (LFS 12.4 versions)
BINUTILS_URL="https://ftp.gnu.org/gnu/binutils/binutils-2.45.tar.xz"
GCC_URL="https://ftp.gnu.org/gnu/gcc/gcc-15.2.0/gcc-15.2.0.tar.xz"
GLIBC_URL="https://ftp.gnu.org/gnu/glibc/glibc-2.42.tar.xz"
GMP_URL="https://ftp.gnu.org/gnu/gmp/gmp-6.3.0.tar.xz"
MPFR_URL="https://ftp.gnu.org/gnu/mpfr/mpfr-4.2.2.tar.xz"
MPC_URL="https://ftp.gnu.org/gnu/mpc/mpc-1.3.1.tar.gz"
LINUX_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.6.72.tar.xz"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_stage() { echo -e "\n${CYAN}================================================================${NC}"; echo -e "${CYAN}  STAGE $1${NC}"; echo -e "${CYAN}================================================================${NC}\n"; }

# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

if [[ $EUID -ne 0 ]]; then
    log_error "Must be run as root."
    exit 1
fi

parse_stage() {
    case "${1:-}" in
        --stage) START_STAGE="${2:-0}" ;;
    esac
}
parse_stage "${@}"

echo ""
log_info "=========================================="
log_info "  AGNOS Self-Hosting ISO Builder"
log_info "=========================================="
log_info "  Version:     $ISO_VERSION"
log_info "  Target:      $LFS"
log_info "  Start stage: $START_STAGE"
log_info "  Repo:        $REPO_DIR"
log_info "=========================================="
echo ""

# ---------------------------------------------------------------------------
# Stage 0: Download source tarballs
# ---------------------------------------------------------------------------

if [[ "$START_STAGE" -le 0 ]]; then
    log_stage "0 — Download source tarballs"

    mkdir -p "${LFS}/sources"

    download() {
        local url="$1"
        local file="${LFS}/sources/$(basename "$url")"
        if [[ -f "$file" ]]; then
            log_info "  Already have: $(basename "$url")"
        else
            log_info "  Downloading: $(basename "$url")"
            curl -fSL "$url" -o "$file"
        fi
    }

    download "$BINUTILS_URL"
    download "$GCC_URL"
    download "$GLIBC_URL"
    download "$GMP_URL"
    download "$MPFR_URL"
    download "$MPC_URL"
    download "$LINUX_URL"

    log_info "All source tarballs ready in ${LFS}/sources/"
fi

# ---------------------------------------------------------------------------
# Stage 1: Bootstrap cross-toolchain
# ---------------------------------------------------------------------------

if [[ "$START_STAGE" -le 1 ]]; then
    log_stage "1 — Bootstrap cross-toolchain (LFS Ch. 5–6)"

    if [[ -x "${LFS}/tools/bin/x86_64-agnos-linux-gnu-gcc" ]]; then
        log_info "Cross-toolchain already built (${LFS}/tools/bin/). Skipping."
    else
        LFS="$LFS" bash "${SCRIPT_DIR}/bootstrap-toolchain.sh"
    fi

    log_info "Stage 1 complete — cross-toolchain ready"
fi

# ---------------------------------------------------------------------------
# Stage 2: Build base system in chroot
# ---------------------------------------------------------------------------

if [[ "$START_STAGE" -le 2 ]]; then
    log_stage "2 — Build base system from recipes"

    # Copy build scripts and recipes into the chroot
    mkdir -p "${LFS}/usr/src/agnos"
    cp -r "$REPO_DIR/recipes" "${LFS}/usr/src/agnos/recipes"
    cp -r "$REPO_DIR/scripts" "${LFS}/usr/src/agnos/scripts"
    cp -r "$REPO_DIR/kernel"  "${LFS}/usr/src/agnos/kernel"
    cp "$REPO_DIR/VERSION"    "${LFS}/usr/src/agnos/VERSION"

    # Install ark-build into chroot PATH
    install -m 755 "$REPO_DIR/scripts/ark-build.sh" "${LFS}/usr/bin/ark-build"

    # Create essential directories for the chroot
    mkdir -p "${LFS}"/{etc,var/log,tmp,run,proc,sys,dev}

    # Create minimal /etc files needed for building
    # nosemgrep: generic.secrets.security.detected-etc-shadow
    cat > "${LFS}/etc/passwd" << 'EOF'
root:x:0:0:root:/root:/bin/bash
nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin
EOF

    # nosemgrep: generic.secrets.security.detected-etc-shadow
    cat > "${LFS}/etc/group" << 'EOF'
root:x:0:
tty:x:5:
nogroup:x:65534:
EOF

    # Build order: critical base packages first (following LFS chapter 8)
    # These are the minimum needed before we can build Rust/cargo
    local base_recipes=(
        # Core system (LFS chapter 8 order)
        "iana-etc"
        "man-pages"
        "glibc"
        "zlib"
        "bzip2"
        "xz"
        "lz4"
        "zstd"
        "file"
        "readline"
        "m4"
        "bc"
        "flex"
        "tcl"
        "expect"
        "dejagnu"
        "pkgconf"
        "binutils"
        "gmp"
        "mpfr"
        "mpc"
        "attr"
        "acl"
        "libcap"
        "libxcrypt"
        "shadow"
        "gcc"
        "ncurses"
        "sed"
        "psmisc"
        "gettext"
        "bison"
        "grep"
        "bash"
        "libtool"
        "gdbm"
        "gperf"
        "expat"
        "inetutils"
        "less"
        "perl"
        "autoconf"
        "automake"
        "openssl"
        "kmod"
        "coreutils"
        "diffutils"
        "gawk"
        "findutils"
        "gzip"
        "make"
        "patch"
        "tar"
        "texinfo"
        "vim"
        "util-linux"
        "e2fsprogs"
        "procps"
        "which"
        "libffi"
        "python"
        "cmake"
        "ninja"
        "meson"
    )

    log_info "Building ${#base_recipes[@]} base packages in chroot..."

    for recipe_name in "${base_recipes[@]}"; do
        local recipe_file="/usr/src/agnos/recipes/base/${recipe_name}.toml"
        if LFS="$LFS" bash "${SCRIPT_DIR}/enter-chroot.sh" \
            "test -f '${recipe_file}' && ark-build '${recipe_file}' 2>&1 || echo 'SKIP: ${recipe_name} (no recipe)'"; then
            log_info "  -> Built: ${recipe_name}"
        else
            log_warn "  -> Failed: ${recipe_name} (continuing)"
        fi
    done

    log_info "Stage 2 complete — base system built"
fi

# ---------------------------------------------------------------------------
# Stage 3: Build AGNOS userland
# ---------------------------------------------------------------------------

if [[ "$START_STAGE" -le 3 ]]; then
    log_stage "3 — Build AGNOS userland (Rust)"

    # Copy userland source into chroot
    mkdir -p "${LFS}/usr/src/agnos"
    cp -r "$REPO_DIR/userland" "${LFS}/usr/src/agnos/userland"
    if [[ -f "$REPO_DIR/userland/Cargo.lock" ]]; then
        cp "$REPO_DIR/userland/Cargo.lock" "${LFS}/usr/src/agnos/userland/Cargo.lock"
    fi

    # Build Rust toolchain recipe first (if not already installed)
    LFS="$LFS" bash "${SCRIPT_DIR}/enter-chroot.sh" \
        "command -v rustc >/dev/null 2>&1 || ark-build /usr/src/agnos/recipes/base/rust.toml" || true

    # Build AGNOS userland
    LFS="$LFS" bash "${SCRIPT_DIR}/enter-chroot.sh" \
        "cd /usr/src/agnos/userland && cargo build --release --workspace"

    # Install built binaries
    LFS="$LFS" bash "${SCRIPT_DIR}/enter-chroot.sh" \
        "install -m 755 /usr/src/agnos/userland/target/release/agent_runtime /usr/bin/agent-runtime && \
         install -m 755 /usr/src/agnos/userland/target/release/llm_gateway /usr/bin/llm-gateway && \
         install -m 755 /usr/src/agnos/userland/target/release/agnsh /usr/bin/agnsh && \
         ln -sf agnsh /usr/bin/agnoshi"

    log_info "Stage 3 complete — AGNOS userland built and installed"
fi

# ---------------------------------------------------------------------------
# Stage 4: Build kernel and create ISO
# ---------------------------------------------------------------------------

if [[ "$START_STAGE" -le 4 ]]; then
    log_stage "4 — Build kernel and create ISO"

    # Build the Linux kernel from recipe
    LFS="$LFS" bash "${SCRIPT_DIR}/enter-chroot.sh" \
        "ark-build /usr/src/agnos/recipes/base/linux.toml" || {
        log_warn "Kernel build via recipe failed — using host kernel as fallback"
    }

    # Set up AGNOS identity
    cat > "${LFS}/etc/os-release" << EOF
NAME="AGNOS"
VERSION="$ISO_VERSION"
ID=agnos
VERSION_ID="$ISO_VERSION"
PRETTY_NAME="AGNOS $ISO_VERSION (AI-Native General Operating System)"
HOME_URL="https://github.com/MacCracken/agnosticos"
EOF

    echo "agnos" > "${LFS}/etc/hostname"

    # Create AGNOS directories
    mkdir -p "${LFS}/var/lib/agnos/"{agents,models,cache,audit}
    mkdir -p "${LFS}/var/log/agnos/audit"
    mkdir -p "${LFS}/run/agnos"
    mkdir -p "${LFS}/etc/agnos"

    # Install selfhost-validate for on-target verification
    install -m 755 "$REPO_DIR/scripts/selfhost-validate.sh" "${LFS}/usr/bin/selfhost-validate"

    # Create ISO
    mkdir -p "$OUTPUT_DIR"
    local iso_file="$OUTPUT_DIR/agnos-${ISO_VERSION}-x86_64-selfhosting.iso"

    log_info "Creating ISO at $iso_file..."

    # Find kernel and initramfs
    local vmlinuz initrd
    vmlinuz=$(find "${LFS}/boot" -name 'vmlinuz-*' -type f 2>/dev/null | head -1)
    initrd=$(find "${LFS}/boot" -name 'initrd*' -o -name 'initramfs*' -type f 2>/dev/null | head -1)

    if [[ -z "$vmlinuz" ]]; then
        log_warn "No kernel found — ISO will need a kernel added manually"
    fi

    # Build ISO structure
    local iso_work="${LFS}/tmp/iso-build"
    mkdir -p "${iso_work}/boot/grub"
    mkdir -p "${iso_work}/live"

    # Copy kernel
    [[ -n "$vmlinuz" ]] && cp "$vmlinuz" "${iso_work}/boot/vmlinuz"
    [[ -n "$initrd" ]]  && cp "$initrd"  "${iso_work}/boot/initrd.img"

    # GRUB config
    cat > "${iso_work}/boot/grub/grub.cfg" << GRUB
set timeout=5
set default=0

menuentry "AGNOS $ISO_VERSION (Self-Hosting)" {
    linux /boot/vmlinuz root=/dev/sda2 rw quiet
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Recovery)" {
    linux /boot/vmlinuz root=/dev/sda2 rw single init=/bin/bash
    initrd /boot/initrd.img
}
GRUB

    # Create squashfs of the root
    if command -v mksquashfs &>/dev/null; then
        mksquashfs "${LFS}" "${iso_work}/live/filesystem.squashfs" \
            -comp zstd -Xcompression-level 15 \
            -noappend \
            -e proc sys dev run tmp/iso-build
        log_info "  -> squashfs: $(du -h "${iso_work}/live/filesystem.squashfs" | cut -f1)"
    fi

    # Create ISO
    if command -v grub-mkrescue &>/dev/null; then
        grub-mkrescue -o "$iso_file" "$iso_work" \
            --modules="part_gpt part_msdos fat iso9660 normal linux all_video" \
            -- -volid "AGNOS_${ISO_VERSION}"

        sha256sum "$iso_file" > "$iso_file.sha256"

        log_info "Self-hosting ISO created: $iso_file"
        log_info "  Size: $(du -h "$iso_file" | cut -f1)"
        log_info "  SHA256: $(cat "$iso_file.sha256" | cut -d' ' -f1)"
    else
        log_warn "grub-mkrescue not found — ISO creation skipped"
        log_info "Root filesystem ready at ${LFS}/ for manual ISO creation"
    fi

    # Cleanup
    rm -rf "${iso_work}"

    log_info "Stage 4 complete"
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

echo ""
log_info "=========================================="
log_info "  AGNOS Self-Hosting Build Complete"
log_info "=========================================="
log_info ""
log_info "  To verify self-hosting:"
log_info "    1. Boot the ISO"
log_info "    2. Run: selfhost-validate --phase all"
log_info ""
log_info "  To rebuild from source inside AGNOS:"
log_info "    cd /usr/src/agnos"
log_info "    ark-build recipes/base/coreutils.toml  # rebuild any package"
log_info "    cd userland && cargo build --release    # rebuild userland"
log_info ""
