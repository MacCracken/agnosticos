#!/bin/bash
# build-iso.sh - Build AGNOS bootable ISO image
#
# Creates a bootable ISO with Debian Trixie base + AGNOS userland.
# Works on both bare metal and QEMU/VirtualBox/VMware.
#
# Requirements: debootstrap, squashfs-tools, grub, libisoburn (xorriso), mtools
# Must be run as root (or with sudo) for debootstrap.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/.."
WORK_DIR="$REPO_DIR/build/iso"
OUTPUT_DIR="$REPO_DIR/output"
CONFIG_DIR="$REPO_DIR/config"
USERLAND_DIR="$REPO_DIR/userland"

# Defaults
ISO_NAME="agnos"
ISO_VERSION="$(cat "${REPO_DIR}/VERSION" 2>/dev/null || echo 'dev')"
ARCH="x86_64"
DEBIAN_ARCH="amd64"
DEBIAN_SUITE="trixie"
DEBIAN_MIRROR="http://deb.debian.org/debian"
SKIP_BUILD=0
SKIP_DEBOOTSTRAP=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step()  { echo -e "${CYAN}[STEP]${NC} $1"; }

usage() {
    cat << EOF
Usage: $0 [options]

Build AGNOS bootable ISO image (Debian Trixie base + AGNOS userland)

Options:
    -n, --name NAME         ISO name (default: agnos)
    -v, --version VERSION   AGNOS version (default: from VERSION file)
    -a, --arch ARCH         Target architecture (default: x86_64)
    -o, --output DIR        Output directory (default: output/)
    -m, --mirror URL        Debian mirror (default: $DEBIAN_MIRROR)
    --skip-build            Skip cargo build (use existing binaries)
    --skip-debootstrap      Skip debootstrap (use existing rootfs)
    -h, --help              Show this help message

Examples:
    sudo $0                         # Full build
    sudo $0 --skip-build            # Rebuild ISO without recompiling
    sudo $0 --skip-debootstrap      # Rebuild with new binaries, keep rootfs
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -n|--name)      ISO_NAME="$2"; shift 2 ;;
            -v|--version)   ISO_VERSION="$2"; shift 2 ;;
            -a|--arch)      ARCH="$2"; shift 2 ;;
            -o|--output)    OUTPUT_DIR="$2"; shift 2 ;;
            -m|--mirror)    DEBIAN_MIRROR="$2"; shift 2 ;;
            --skip-build)   SKIP_BUILD=1; shift ;;
            --skip-debootstrap) SKIP_DEBOOTSTRAP=1; shift ;;
            -h|--help)      usage; exit 0 ;;
            *)              log_error "Unknown option: $1"; usage; exit 1 ;;
        esac
    done
}

check_requirements() {
    log_step "Checking requirements..."

    local missing=()
    for cmd in debootstrap mksquashfs grub-mkrescue xorriso; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        log_info "Install with: sudo pacman -S squashfs-tools grub libisoburn mtools debootstrap debian-archive-keyring"
        exit 1
    fi

    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (needed for debootstrap and chroot)"
        log_info "Run with: sudo $0 $*"
        exit 1
    fi
}

setup_directories() {
    log_step "Setting up build directories..."

    mkdir -p "$WORK_DIR"
    mkdir -p "$WORK_DIR/iso/boot/grub"
    mkdir -p "$WORK_DIR/iso/live"
    mkdir -p "$OUTPUT_DIR"
}

build_userland() {
    if [[ $SKIP_BUILD -eq 1 ]]; then
        log_info "Skipping cargo build (--skip-build)"
        if [[ ! -d "$USERLAND_DIR/target/x86_64-unknown-linux-musl/release" ]] && \
           [[ ! -d "$USERLAND_DIR/target/release" ]]; then
            log_error "No release binaries found. Run without --skip-build first."
            exit 1
        fi
        return
    fi

    log_step "Building AGNOS userland (release, musl static)..."

    # Build as the original user if running under sudo
    # sudo strips PATH, so resolve cargo directly
    local cargo_bin
    local build_args="build --release --target x86_64-unknown-linux-musl --manifest-path $USERLAND_DIR/Cargo.toml"
    if [[ -n "${SUDO_USER:-}" ]]; then
        local user_home
        user_home=$(getent passwd "$SUDO_USER" | cut -d: -f6)
        cargo_bin="${user_home}/.cargo/bin/cargo"
        if [[ ! -x "$cargo_bin" ]]; then
            log_error "cargo not found at $cargo_bin"
            log_info "Either install rustup for $SUDO_USER or build first:"
            log_info "  cargo build --release --target x86_64-unknown-linux-musl --manifest-path userland/Cargo.toml"
            log_info "Then re-run with: sudo $0 --skip-build"
            exit 1
        fi
        sudo -u "$SUDO_USER" \
            HOME="$user_home" \
            PATH="${user_home}/.cargo/bin:${PATH}" \
            "$cargo_bin" $build_args
    else
        cargo $build_args
    fi

    log_info "  -> Userland build complete"
}

create_rootfs() {
    local rootfs="$WORK_DIR/rootfs"

    if [[ $SKIP_DEBOOTSTRAP -eq 1 ]] && [[ -d "$rootfs/bin" ]]; then
        log_info "Skipping debootstrap (--skip-debootstrap)"
    else
        log_step "Bootstrapping Debian $DEBIAN_SUITE base system..."

        # Clean previous rootfs
        rm -rf "$rootfs"

        debootstrap --variant=minbase --arch="$DEBIAN_ARCH" \
            --include=systemd,systemd-sysv,dbus,udev,iproute2,iputils-ping,kmod,procps,openssh-server,sudo,vim-tiny,linux-image-amd64 \
            "$DEBIAN_SUITE" "$rootfs" "$DEBIAN_MIRROR"

        # Install live-boot for squashfs-based live booting
        chroot "$rootfs" /usr/bin/env PATH=/usr/sbin:/usr/bin:/sbin:/bin /bin/bash -c "
            apt-get update
            apt-get install -y --no-install-recommends live-boot
            apt-get clean
            rm -rf /var/lib/apt/lists/*
        "

        # Rebuild initramfs with live-boot support
        chroot "$rootfs" /usr/bin/env PATH=/usr/sbin:/usr/bin:/sbin:/bin /bin/bash -c "
            update-initramfs -u
        "

        log_info "  -> Base system bootstrapped with live-boot"
    fi

    log_step "Configuring AGNOS rootfs..."

    # --- Hostname & identity ---
    echo "agnos" > "$rootfs/etc/hostname"
    cat > "$rootfs/etc/hosts" << 'EOF'
127.0.0.1   localhost agnos
::1         localhost agnos
EOF

    # --- OS release ---
    cat > "$rootfs/etc/os-release" << EOF
NAME="AGNOS"
VERSION="$ISO_VERSION"
ID=agnos
ID_LIKE=debian
VERSION_ID="$ISO_VERSION"
PRETTY_NAME="AGNOS $ISO_VERSION (AI-Native General Operating System)"
HOME_URL="https://github.com/agnos/agnos"
BUG_REPORT_URL="https://github.com/agnos/agnos/issues"
EOF

    # --- Users ---
    chroot "$rootfs" /usr/bin/env PATH=/usr/sbin:/usr/bin:/sbin:/bin /bin/bash -c "
        # Create agnos system users
        groupadd -f agnos
        useradd -r -g agnos -d /var/lib/agnos -s /usr/sbin/nologin agnos 2>/dev/null || true
        groupadd -f agnos-llm
        useradd -r -g agnos-llm -d /var/lib/agnos/models -s /usr/sbin/nologin agnos-llm 2>/dev/null || true

        # Create default user account (password: agnos)
        useradd -m -G sudo -s /bin/bash user 2>/dev/null || true
        echo 'user:agnos' | chpasswd

        # Root password (password: agnos)
        echo 'root:agnos' | chpasswd
    "

    # --- AGNOS directories ---
    mkdir -p "$rootfs/var/lib/agnos/"{agents,models,cache,audit}
    mkdir -p "$rootfs/var/log/agnos/audit"
    mkdir -p "$rootfs/run/agnos"
    mkdir -p "$rootfs/etc/agnos"
    mkdir -p "$rootfs/usr/lib/agnos/init"

    # --- Install AGNOS binaries ---
    log_step "Installing AGNOS binaries..."

    # Prefer musl static binaries, fall back to regular release
    local release_dir="$USERLAND_DIR/target/x86_64-unknown-linux-musl/release"
    if [[ ! -d "$release_dir" ]]; then
        release_dir="$USERLAND_DIR/target/release"
        log_warn "  -> No musl binaries found, using glibc build (may have compatibility issues)"
    fi

    # Map: source_binary -> installed_name
    declare -A binaries=(
        [agent_runtime]=agent-runtime
        [llm_gateway]=llm-gateway
        [agnsh]=agnsh
        [agnos-sudo]=agnos-sudo
    )

    for src in "${!binaries[@]}"; do
        local dest="${binaries[$src]}"
        if [[ -f "$release_dir/$src" ]]; then
            cp "$release_dir/$src" "$rootfs/usr/bin/$dest"
            chmod 755 "$rootfs/usr/bin/$dest"
            log_info "  -> Installed $dest ($(du -h "$release_dir/$src" | cut -f1))"
        else
            log_warn "  -> Binary not found: $src (skipping)"
        fi
    done

    # Symlink agnoshi -> agnsh
    ln -sf agnsh "$rootfs/usr/bin/agnoshi"

    # --- Systemd units ---
    log_step "Installing systemd units..."

    if [[ -d "$CONFIG_DIR/systemd/system" ]]; then
        cp "$CONFIG_DIR/systemd/system/"*.service "$rootfs/etc/systemd/system/" 2>/dev/null || true
        # Enable AGNOS services
        chroot "$rootfs" /usr/bin/env PATH=/usr/sbin:/usr/bin:/sbin:/bin /bin/bash -c "
            systemctl enable agnos-init.service 2>/dev/null || true
            systemctl enable agent-runtime.service 2>/dev/null || true
            systemctl enable llm-gateway.service 2>/dev/null || true
        "
        log_info "  -> Systemd units installed and enabled"
    fi

    # --- Init scripts ---
    if [[ -d "$CONFIG_DIR/init" ]]; then
        cp "$CONFIG_DIR/init/"*.sh "$rootfs/usr/lib/agnos/init/"
        chmod +x "$rootfs/usr/lib/agnos/init/"*.sh
        log_info "  -> Init scripts installed"
    fi

    # --- Sysctl hardening ---
    if [[ -f "$CONFIG_DIR/sysctl/99-agnos-hardening.conf" ]]; then
        mkdir -p "$rootfs/etc/sysctl.d"
        cp "$CONFIG_DIR/sysctl/99-agnos-hardening.conf" "$rootfs/etc/sysctl.d/"
        log_info "  -> Sysctl hardening installed"
    fi

    # --- Service configs ---
    if [[ -d "$CONFIG_DIR/services" ]]; then
        cp "$CONFIG_DIR/services/"*.toml "$rootfs/etc/agnos/" 2>/dev/null || true
        log_info "  -> Service configs installed"
    fi

    # --- Networking (systemd-networkd) ---
    mkdir -p "$rootfs/etc/systemd/network"
    cat > "$rootfs/etc/systemd/network/20-wired.network" << 'EOF'
[Match]
Name=en* eth*

[Network]
DHCP=yes
EOF

    chroot "$rootfs" /usr/bin/env PATH=/usr/sbin:/usr/bin:/sbin:/bin /bin/bash -c "
        systemctl enable systemd-networkd 2>/dev/null || true
        systemctl enable systemd-resolved 2>/dev/null || true
        systemctl enable ssh 2>/dev/null || true
        # Disable socket-activated SSH (Trixie default) — we want sshd listening on TCP
        systemctl disable sshd-unix-local.socket 2>/dev/null || true
        systemctl disable sshd-vsock.socket 2>/dev/null || true
        systemctl enable serial-getty@ttyS0.service 2>/dev/null || true
    "

    # --- Ensure sshd listens on TCP port 22 ---
    mkdir -p "$rootfs/etc/ssh/sshd_config.d"
    cat > "$rootfs/etc/ssh/sshd_config.d/10-agnos.conf" << 'EOF'
ListenAddress 0.0.0.0
PasswordAuthentication yes
PermitRootLogin yes
EOF

    # --- MOTD ---
    cat > "$rootfs/etc/motd" << EOF

    ___    _____ _   ______  _____
   /   |  / ___// | / / __ \/ ___/
  / /| | / __ \/  |/ / / / /\__ \\
 / ___ |/ /_/ / /|  / /_/ /___/ /
/_/  |_|\____/_/ |_/\____//____/

AI-Native General Operating System v${ISO_VERSION}
https://github.com/agnos/agnos

Default credentials: user/agnos  root/agnos

EOF

    # --- Permissions ---
    chroot "$rootfs" /usr/bin/env PATH=/usr/sbin:/usr/bin:/sbin:/bin /bin/bash -c "
        chown -R agnos:agnos /var/lib/agnos/agents 2>/dev/null || true
        chown -R agnos-llm:agnos-llm /var/lib/agnos/models 2>/dev/null || true
        chmod 750 /var/log/agnos
    "

    # --- Cleanup ---
    chroot "$rootfs" /usr/bin/env PATH=/usr/sbin:/usr/bin:/sbin:/bin /bin/bash -c "
        apt-get clean
        rm -rf /var/lib/apt/lists/*
        rm -rf /tmp/*
    "

    log_info "  -> Rootfs configured"
}

create_squashfs() {
    log_step "Creating squashfs root filesystem..."

    local rootfs="$WORK_DIR/rootfs"
    local squashfs="$WORK_DIR/iso/live/filesystem.squashfs"

    rm -f "$squashfs"

    mksquashfs "$rootfs" "$squashfs" \
        -comp zstd -Xcompression-level 15 \
        -noappend \
        -e boot/vmlinuz* boot/initrd* \
        || {
        log_error "Failed to create squashfs"
        exit 1
    }

    log_info "  -> Root filesystem: $(du -h "$squashfs" | cut -f1)"
}

setup_kernel() {
    log_step "Setting up kernel..."

    local rootfs="$WORK_DIR/rootfs"

    # Use the kernel installed by debootstrap (linux-image-amd64)
    local vmlinuz=$(find "$rootfs/boot" -name 'vmlinuz-*' -type f | head -1)
    local initrd=$(find "$rootfs/boot" -name 'initrd.img-*' -type f | head -1)

    if [[ -z "$vmlinuz" ]]; then
        log_error "No kernel found in rootfs. debootstrap may have failed."
        exit 1
    fi

    cp "$vmlinuz" "$WORK_DIR/iso/boot/vmlinuz"
    cp "$initrd" "$WORK_DIR/iso/boot/initrd.img"

    local kver=$(basename "$vmlinuz" | sed 's/vmlinuz-//')
    log_info "  -> Kernel: $kver"
    log_info "  -> vmlinuz: $(du -h "$vmlinuz" | cut -f1)"
    log_info "  -> initrd: $(du -h "$initrd" | cut -f1)"
}

create_grub_config() {
    log_step "Creating GRUB configuration..."

    cat > "$WORK_DIR/iso/boot/grub/grub.cfg" << EOF
set timeout=5
set default=0

insmod all_video
insmod gfxterm
insmod png

if loadfont /boot/grub/fonts/unicode.pf2 ; then
    set gfxmode=auto
    terminal_output gfxterm
fi

menuentry "AGNOS $ISO_VERSION" {
    linux /boot/vmlinuz boot=live toram quiet loglevel=3 net.ifnames=0
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Live - no toram)" {
    linux /boot/vmlinuz boot=live quiet loglevel=3 net.ifnames=0
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Debug)" {
    linux /boot/vmlinuz boot=live debug loglevel=7 systemd.log_level=debug net.ifnames=0
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Serial Console)" {
    linux /boot/vmlinuz boot=live console=ttyS0,115200n8 console=tty0 loglevel=5 net.ifnames=0
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Recovery Shell)" {
    linux /boot/vmlinuz boot=live single init=/bin/bash net.ifnames=0
    initrd /boot/initrd.img
}
EOF
}

create_iso() {
    log_step "Creating ISO image..."

    local iso_file="$OUTPUT_DIR/${ISO_NAME}-${ISO_VERSION}-${ARCH}.iso"

    grub-mkrescue -o "$iso_file" "$WORK_DIR/iso" \
        --modules="part_gpt part_msdos fat iso9660 normal linux all_video" \
        -- -volid "AGNOS_${ISO_VERSION}" \
        || {
        log_error "grub-mkrescue failed"
        exit 1
    }

    # Checksums
    sha256sum "$iso_file" > "$iso_file.sha256"

    log_info "ISO created: $iso_file"
    log_info "  Size: $(du -h "$iso_file" | cut -f1)"
    log_info "  SHA256: $(cat "$iso_file.sha256" | cut -d' ' -f1)"

    echo ""
    log_info "To test with QEMU:"
    log_info "  qemu-system-x86_64 -m 2G -smp 2 -enable-kvm -cdrom $iso_file -boot d"
    log_info ""
    log_info "With serial console:"
    log_info "  qemu-system-x86_64 -m 2G -smp 2 -enable-kvm -cdrom $iso_file -boot d -nographic -append 'console=ttyS0'"
    log_info ""
    log_info "With port forwarding (SSH on 2222, daimon on 8090, hoosh on 8088):"
    log_info "  qemu-system-x86_64 -m 2G -smp 2 -enable-kvm -cdrom $iso_file -boot d \\"
    log_info "    -nic user,hostfwd=tcp::2222-:22,hostfwd=tcp::18090-:8090,hostfwd=tcp::18088-:8088"
}

main() {
    parse_args "$@"

    log_info "=========================================="
    log_info "  AGNOS ISO Builder"
    log_info "=========================================="
    log_info "  Version:  $ISO_VERSION"
    log_info "  Arch:     $ARCH"
    log_info "  Base:     Debian $DEBIAN_SUITE"
    log_info "  Output:   $OUTPUT_DIR/"
    log_info "=========================================="
    echo ""

    check_requirements
    setup_directories
    build_userland
    create_rootfs
    setup_kernel
    create_squashfs
    create_grub_config
    create_iso

    echo ""
    log_info "Build complete!"
}

main "$@"
