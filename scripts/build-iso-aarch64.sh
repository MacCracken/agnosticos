#!/bin/bash
# build-iso-aarch64.sh — Build AGNOS bootable aarch64 SD card image
#
# Creates an AGNOS system (Debian Trixie arm64 base + AGNOS userland)
# bootable on Raspberry Pi 4/5 via microSD.
#
# Profiles:
#   --edge    Minimal edge/IoT image (~512MB) with SY edge agent, WireGuard,
#             no desktop, no GPU. For headless RPi4/5 edge fleet nodes.
#   (default) Full desktop-capable system (~2GB) with all AGNOS binaries.
#
# Output: agnos-<version>-aarch64.img (or agnos-edge-<version>-aarch64.img)
#
# Requirements:
#   debootstrap, squashfs-tools, dosfstools (mkfs.vfat), parted,
#   e2fsprogs (mkfs.ext4), qemu-user-static (for arm64 chroot),
#   debian-archive-keyring
#
# Cross-compilation:
#   Rust userland is cross-compiled with aarch64-unknown-linux-gnu target
#   via Cross.toml (Docker-based cross-compilation).
#
# Must be run as root (or with sudo) for debootstrap, losetup, mount.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/.."
WORK_DIR="$REPO_DIR/build/iso-aarch64"
OUTPUT_DIR="$REPO_DIR/output"
CONFIG_DIR="$REPO_DIR/config"
USERLAND_DIR="$REPO_DIR/userland"

# Defaults
ISO_NAME="agnos"
ISO_VERSION="$(cat "${REPO_DIR}/VERSION" 2>/dev/null || echo 'dev')"
ARCH="aarch64"
DEBIAN_ARCH="arm64"
DEBIAN_SUITE="trixie"
DEBIAN_MIRROR="http://deb.debian.org/debian"
SKIP_BUILD=0
SKIP_DEBOOTSTRAP=0
EDGE_MODE=0

# Image sizing — adjusted by profile (--edge shrinks to 512MB)
IMG_SIZE_MB=2048
BOOT_SIZE_MB=256    # FAT32 boot: kernel, DTBs, firmware, initrd
# Remaining space for ext4 rootfs

# SY edge binary path (set via --sy-edge-binary or auto-detected)
SY_EDGE_BINARY=""

# Global rootfs path (set by create_rootfs, used by install_edge_packages and cleanup)
ROOTFS=""

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

Build AGNOS bootable aarch64 SD card image (Debian Trixie arm64 + AGNOS userland)

Flash to microSD with:
  sudo dd if=output/agnos-<version>-aarch64.img of=/dev/sdX bs=4M status=progress conv=fsync

Options:
    -n, --name NAME         Image name (default: agnos)
    -v, --version VERSION   AGNOS version (default: from VERSION file)
    -o, --output DIR        Output directory (default: output/)
    -s, --size MB           Image size in MB (default: 2048, edge: 512)
    -m, --mirror URL        Debian mirror (default: $DEBIAN_MIRROR)
    --edge                  Build minimal edge/IoT image with SY agent
    --sy-edge-binary PATH   Path to secureyeoman-edge arm64 binary
    --skip-build            Skip cross-compilation (use existing binaries)
    --skip-debootstrap      Skip debootstrap (use existing rootfs)
    -h, --help              Show this help message

Examples:
    sudo $0                         # Full desktop build (2GB)
    sudo $0 --edge                  # Minimal edge image (512MB)
    sudo $0 --edge --sy-edge-binary /path/to/secureyeoman-edge-linux-arm64
    sudo $0 --skip-build            # Rebuild image without recompiling
    sudo $0 --size 4096             # 4 GB image (more room for models)
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -n|--name)      ISO_NAME="$2"; shift 2 ;;
            -v|--version)   ISO_VERSION="$2"; shift 2 ;;
            -o|--output)    OUTPUT_DIR="$2"; shift 2 ;;
            -s|--size)      IMG_SIZE_MB="$2"; shift 2 ;;
            -m|--mirror)    DEBIAN_MIRROR="$2"; shift 2 ;;
            --edge)         EDGE_MODE=1; shift ;;
            --sy-edge-binary) SY_EDGE_BINARY="$2"; shift 2 ;;
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
    for cmd in debootstrap mksquashfs parted mkfs.vfat mkfs.ext4 losetup; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done

    # qemu-user-static is needed for arm64 chroot on x86_64 host
    if [[ "$(uname -m)" != "aarch64" ]]; then
        if ! command -v qemu-aarch64-static &>/dev/null && \
           [[ ! -f /usr/bin/qemu-aarch64-static ]] && \
           [[ ! -f /proc/sys/fs/binfmt_misc/qemu-aarch64 ]]; then
            missing+=("qemu-user-static")
        fi
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        log_info "Install with: sudo pacman -S squashfs-tools dosfstools parted e2fsprogs qemu-user-static qemu-user-static-binfmt debootstrap debian-archive-keyring"
        exit 1
    fi

    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (needed for debootstrap, losetup, mount)"
        log_info "Run with: sudo $0 $*"
        exit 1
    fi
}

setup_directories() {
    log_step "Setting up build directories..."
    mkdir -p "$WORK_DIR"
    mkdir -p "$OUTPUT_DIR"
}

build_userland() {
    if [[ $SKIP_BUILD -eq 1 ]]; then
        log_info "Skipping cross-compilation (--skip-build)"
        if [[ ! -d "$USERLAND_DIR/target/aarch64-unknown-linux-gnu/release" ]]; then
            if [[ $EDGE_MODE -eq 1 ]]; then
                log_warn "No AGNOS aarch64 binaries — edge image will use SY edge binary only"
                mkdir -p "$USERLAND_DIR/target/aarch64-unknown-linux-gnu/release"
                return
            fi
            log_error "No aarch64 release binaries found. Run without --skip-build first."
            exit 1
        fi
        return
    fi

    log_step "Cross-compiling AGNOS userland for aarch64..."

    # Use cross for Docker-based cross-compilation (configured in Cross.toml)
    local build_cmd="cross"
    local build_args="build --release --target aarch64-unknown-linux-gnu --manifest-path $USERLAND_DIR/Cargo.toml"

    if [[ -n "${SUDO_USER:-}" ]]; then
        local user_home
        user_home=$(getent passwd "$SUDO_USER" | cut -d: -f6)

        # Try cross first (Docker-based, handles all deps)
        local cross_bin="${user_home}/.cargo/bin/cross"
        local cargo_bin="${user_home}/.cargo/bin/cargo"

        if [[ -x "$cross_bin" ]]; then
            log_info "  -> Using cross (Docker-based cross-compilation)"
            sudo -u "$SUDO_USER" \
                HOME="$user_home" \
                PATH="${user_home}/.cargo/bin:${PATH}" \
                "$cross_bin" $build_args
        elif [[ -x "$cargo_bin" ]]; then
            log_info "  -> Using cargo with aarch64-unknown-linux-gnu target"
            log_warn "  -> If this fails, install cross: cargo install cross"
            sudo -u "$SUDO_USER" \
                HOME="$user_home" \
                PATH="${user_home}/.cargo/bin:${PATH}" \
                "$cargo_bin" build --release --target aarch64-unknown-linux-gnu \
                --manifest-path "$USERLAND_DIR/Cargo.toml"
        else
            log_error "Neither cross nor cargo found at $user_home/.cargo/bin/"
            log_info "Install cross: cargo install cross"
            exit 1
        fi
    else
        if command -v cross &>/dev/null; then
            cross $build_args
        else
            cargo build --release --target aarch64-unknown-linux-gnu \
                --manifest-path "$USERLAND_DIR/Cargo.toml"
        fi
    fi

    log_info "  -> Userland cross-compilation complete"
}

create_rootfs() {
    ROOTFS="$WORK_DIR/rootfs"
    local rootfs="$ROOTFS"

    if [[ $SKIP_DEBOOTSTRAP -eq 1 ]] && [[ -d "$rootfs/bin" ]]; then
        log_info "Skipping debootstrap (--skip-debootstrap)"
    else
        log_step "Bootstrapping Debian $DEBIAN_SUITE arm64 base system..."

        rm -rf "$rootfs"

        # Package list — edge mode is more minimal
        local include_pkgs="systemd,systemd-sysv,dbus,udev,iproute2,iputils-ping,kmod,procps,openssh-server,sudo,passwd,vim-tiny"
        if [[ $EDGE_MODE -eq 1 ]]; then
            include_pkgs="systemd,systemd-sysv,dbus,udev,iproute2,iputils-ping,kmod,procps,openssh-server,sudo,passwd,ca-certificates,curl"
        fi

        # Foreign debootstrap: first stage downloads packages,
        # second stage unpacks them (requires qemu-user-static for chroot)
        if [[ "$(uname -m)" != "aarch64" ]]; then
            debootstrap --foreign --variant=minbase --arch="$DEBIAN_ARCH" \
                --include="$include_pkgs" \
                "$DEBIAN_SUITE" "$rootfs" "$DEBIAN_MIRROR"

            # Copy qemu-user-static for chroot
            cp /usr/bin/qemu-aarch64-static "$rootfs/usr/bin/"

            # Second stage inside chroot
            chroot "$rootfs" /usr/bin/qemu-aarch64-static /bin/bash -c "/debootstrap/debootstrap --second-stage"
        else
            # Native aarch64 host — no foreign bootstrap needed
            debootstrap --variant=minbase --arch="$DEBIAN_ARCH" \
                --include="$include_pkgs" \
                "$DEBIAN_SUITE" "$rootfs" "$DEBIAN_MIRROR"
        fi

        log_info "  -> Base system bootstrapped"
    fi

    log_step "Configuring AGNOS rootfs (aarch64)..."

    # Mount pseudo-filesystems for chroot (needed by groupadd, apt, etc.)
    mount -t proc proc "$rootfs/proc" 2>/dev/null || true
    mount -t sysfs sysfs "$rootfs/sys" 2>/dev/null || true
    mount -t devpts devpts "$rootfs/dev/pts" -o ptmxmode=0666,newinstance 2>/dev/null || true

    # Cleanup pseudo-fs on exit
    cleanup_chroot() {
        local r="${ROOTFS:-}"
        if [[ -n "$r" ]]; then
            umount "$r/dev/pts" 2>/dev/null || true
            umount "$r/sys" 2>/dev/null || true
            umount "$r/proc" 2>/dev/null || true
        fi
    }
    trap 'cleanup_chroot' EXIT

    # Helper for chroot — handles both native and foreign
    run_chroot() {
        if [[ "$(uname -m)" != "aarch64" ]]; then
            chroot "$rootfs" /usr/bin/qemu-aarch64-static /bin/bash -c "$1"
        else
            chroot "$rootfs" /bin/bash -c "$1"
        fi
    }

    # --- Install RPi kernel from Debian ---
    log_step "Installing aarch64 kernel..."
    run_chroot "
        apt-get update
        # linux-image-arm64 is the generic arm64 kernel
        apt-get install -y --no-install-recommends linux-image-arm64
        # RPi WiFi firmware — non-free, may not be available in all mirrors
        apt-get install -y --no-install-recommends firmware-brcm80211 2>/dev/null || true
        apt-get clean
        rm -rf /var/lib/apt/lists/*
    "

    # --- Hostname & identity ---
    local hostname="agnos"
    [[ $EDGE_MODE -eq 1 ]] && hostname="agnos-edge"
    echo "$hostname" > "$rootfs/etc/hostname"
    cat > "$rootfs/etc/hosts" << EOF
127.0.0.1   localhost $hostname
::1         localhost $hostname
EOF

    # --- OS release ---
    cat > "$rootfs/etc/os-release" << EOF
NAME="AGNOS"
VERSION="$ISO_VERSION"
ID=agnos
ID_LIKE=debian
VERSION_ID="$ISO_VERSION"
PRETTY_NAME="AGNOS $ISO_VERSION (AI-Native General Operating System) [aarch64]"
HOME_URL="https://github.com/agnos/agnos"
BUG_REPORT_URL="https://github.com/agnos/agnos/issues"
EOF

    # --- Users ---
    run_chroot "
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

    # --- Install AGNOS binaries (aarch64 cross-compiled) ---
    log_step "Installing AGNOS aarch64 binaries..."

    local release_dir="$USERLAND_DIR/target/aarch64-unknown-linux-gnu/release"
    if [[ ! -d "$release_dir" ]]; then
        log_error "No aarch64 binaries found at $release_dir"
        log_info "Run without --skip-build to cross-compile, or build manually:"
        log_info "  cross build --release --target aarch64-unknown-linux-gnu"
        exit 1
    fi

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

    ln -sf agnsh "$rootfs/usr/bin/agnoshi"

    # --- Systemd units ---
    log_step "Installing systemd units..."

    if [[ -d "$CONFIG_DIR/systemd/system" ]]; then
        cp "$CONFIG_DIR/systemd/system/"*.service "$rootfs/etc/systemd/system/" 2>/dev/null || true
        run_chroot "
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

    run_chroot "
        systemctl enable systemd-networkd 2>/dev/null || true
        systemctl enable systemd-resolved 2>/dev/null || true
        systemctl enable ssh 2>/dev/null || true
        systemctl disable sshd-unix-local.socket 2>/dev/null || true
        systemctl disable sshd-vsock.socket 2>/dev/null || true
        systemctl enable serial-getty@ttyS0.service 2>/dev/null || true
        systemctl enable serial-getty@ttyAMA0.service 2>/dev/null || true
    "

    # --- SSH config ---
    mkdir -p "$rootfs/etc/ssh/sshd_config.d"
    cat > "$rootfs/etc/ssh/sshd_config.d/10-agnos.conf" << 'EOF'
ListenAddress 0.0.0.0
PasswordAuthentication yes
PermitRootLogin yes
EOF

    # --- MOTD ---
    local motd_platform="Raspberry Pi 4/5"
    local motd_profile=""
    if [[ $EDGE_MODE -eq 1 ]]; then
        motd_profile="Edge/IoT Profile — Headless, SY Edge Agent enabled"
    fi
    cat > "$rootfs/etc/motd" << EOF

    ___    _____ _   ______  _____
   /   |  / ___// | / / __ \/ ___/
  / /| | / __ \/  |/ / / / /\__ \\
 / ___ |/ /_/ / /|  / /_/ /___/ /
/_/  |_|\____/_/ |_/\____//____/

AI-Native General Operating System v${ISO_VERSION} [aarch64]
${motd_profile:+$motd_profile
}https://github.com/agnos/agnos

Default credentials: user/agnos  root/agnos
Platform: ${motd_platform}

EOF

    # --- fstab for SD card boot ---
    cat > "$rootfs/etc/fstab" << 'EOF'
# AGNOS aarch64 fstab
# <device>          <mount>     <type>  <options>           <dump> <pass>
/dev/mmcblk0p2      /           ext4    defaults,noatime    0      1
/dev/mmcblk0p1      /boot       vfat    defaults            0      2
tmpfs               /tmp        tmpfs   defaults,nosuid     0      0
tmpfs               /run/agnos  tmpfs   defaults,size=64M   0      0
EOF

    # --- Permissions ---
    run_chroot "
        chown -R agnos:agnos /var/lib/agnos/agents 2>/dev/null || true
        chown -R agnos-llm:agnos-llm /var/lib/agnos/models 2>/dev/null || true
        chmod 750 /var/log/agnos
    "

    # --- Cleanup ---
    run_chroot "
        apt-get clean
        rm -rf /var/lib/apt/lists/*
        rm -rf /tmp/*
    "

    # --- Edge-specific size reduction ---
    if [[ $EDGE_MODE -eq 1 ]]; then
        log_step "Minimizing rootfs for edge deployment..."
        run_chroot "
            # Remove docs, man pages, locale data, includes
            rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/*
            rm -rf /usr/share/locale/* /usr/share/i18n/*
            rm -rf /usr/include/*
            rm -rf /usr/src/*
            rm -rf /usr/games
            # Remove __pycache__ and .pyc
            find / -name __pycache__ -type d -exec rm -rf {} + 2>/dev/null || true
            find / -name '*.pyc' -delete 2>/dev/null || true
            # Remove unnecessary systemd units
            rm -f /usr/lib/systemd/system/systemd-pcrlock* 2>/dev/null || true
            rm -f /usr/lib/systemd/system/systemd-tpm2-setup* 2>/dev/null || true
            rm -f /usr/lib/systemd/system/systemd-quotacheck* 2>/dev/null || true
            rm -f /usr/lib/systemd/system/systemd-pstore* 2>/dev/null || true
            # Strip debug symbols from shared libs
            find /usr/lib -name '*.so*' -exec strip --strip-unneeded {} 2>/dev/null \; || true
        "
        log_info "  -> Edge rootfs minimized"
    fi

    log_info "  -> Rootfs configured"
}

create_image() {
    log_step "Creating ${IMG_SIZE_MB}MB SD card image..."

    local img_out="$OUTPUT_DIR/${ISO_NAME}-${ISO_VERSION}-${ARCH}.img"
    local rootfs="$WORK_DIR/rootfs"
    local loop_dev=""

    # Create empty image file
    dd if=/dev/zero of="$img_out" bs=1M count="$IMG_SIZE_MB" status=none

    # Partition: 256 MB FAT32 boot + remaining ext4 rootfs
    log_info "  Partitioning image..."
    parted -s "$img_out" \
        mklabel msdos \
        mkpart primary fat32 1MiB "${BOOT_SIZE_MB}MiB" \
        mkpart primary ext4 "${BOOT_SIZE_MB}MiB" 100% \
        set 1 boot on

    # Setup loop device
    loop_dev="$(losetup --show -fP "$img_out" 2>/dev/null)" || {
        log_error "losetup failed — are you running as root?"
        rm -f "$img_out"
        exit 1
    }

    # Cleanup trap
    cleanup_loop() {
        if [[ -n "${loop_dev:-}" ]]; then
            umount "$WORK_DIR/mnt_boot" 2>/dev/null || true
            umount "$WORK_DIR/mnt_root" 2>/dev/null || true
            losetup -d "$loop_dev" 2>/dev/null || true
        fi
    }
    trap cleanup_loop EXIT

    # Format
    log_info "  Formatting partitions..."
    mkfs.vfat -F 32 -n "AGNOS-BOOT" "${loop_dev}p1"
    mkfs.ext4 -q -L "AGNOS-ROOT" "${loop_dev}p2"

    # Mount
    mkdir -p "$WORK_DIR/mnt_boot" "$WORK_DIR/mnt_root"
    mount "${loop_dev}p1" "$WORK_DIR/mnt_boot"
    mount "${loop_dev}p2" "$WORK_DIR/mnt_root"

    # --- Populate rootfs ---
    log_step "Copying rootfs to image..."
    cp -a "$rootfs/"* "$WORK_DIR/mnt_root/"
    mkdir -p "$WORK_DIR/mnt_root/boot"

    # --- Populate boot partition ---
    log_step "Setting up RPi boot partition..."

    # Copy kernel and initrd from Debian install
    local vmlinuz=$(find "$rootfs/boot" -name 'vmlinuz-*' -type f | head -1)
    local initrd=$(find "$rootfs/boot" -name 'initrd.img-*' -type f | head -1)
    local kver=""

    if [[ -n "$vmlinuz" ]]; then
        kver=$(basename "$vmlinuz" | sed 's/vmlinuz-//')
        # RPi expects 'kernel8.img' for 64-bit kernel, or use config.txt to specify
        cp "$vmlinuz" "$WORK_DIR/mnt_boot/vmlinuz"
        log_info "  -> Kernel: $kver ($(du -h "$vmlinuz" | cut -f1))"
    else
        log_error "No kernel found in rootfs"
        exit 1
    fi

    if [[ -n "$initrd" ]]; then
        cp "$initrd" "$WORK_DIR/mnt_boot/initrd.img"
        log_info "  -> initrd: $(du -h "$initrd" | cut -f1)"
    fi

    # Copy DTBs
    if [[ -d "$rootfs/usr/lib/linux-image-${kver}" ]]; then
        cp -r "$rootfs/usr/lib/linux-image-${kver}/broadcom" "$WORK_DIR/mnt_boot/" 2>/dev/null || true
        # Also copy overlays subdirectory
        if [[ -d "$rootfs/usr/lib/linux-image-${kver}/overlays" ]]; then
            cp -r "$rootfs/usr/lib/linux-image-${kver}/overlays" "$WORK_DIR/mnt_boot/"
        fi
        log_info "  -> DTBs copied"
    fi

    # RPi boot firmware — config.txt + cmdline.txt
    local gpu_mem=128
    local gpu_overlay="dtoverlay=vc4-kms-v3d"
    if [[ $EDGE_MODE -eq 1 ]]; then
        gpu_mem=16      # Minimal GPU — headless, no compositor
        gpu_overlay=""   # No GPU overlay for edge
    fi
    cat > "$WORK_DIR/mnt_boot/config.txt" << RPICFG
# AGNOS — Raspberry Pi boot configuration

# 64-bit mode
arm_64bit=1

# Kernel (Debian arm64 kernel)
kernel=vmlinuz
initramfs initrd.img followkernel

# Device tree
${gpu_overlay}
dtparam=i2c_arm=on
dtparam=spi=on
dtparam=audio=on

# GPU memory
gpu_mem=${gpu_mem}

# Serial console (useful for headless setup)
enable_uart=1

# HDMI (force hotplug for headless)
hdmi_force_hotplug=1

# Disable splash / rainbow screen
disable_splash=1

# USB boot timeout (RPi4)
boot_delay=1
RPICFG

    cat > "$WORK_DIR/mnt_boot/cmdline.txt" << CMDLINE
console=serial0,115200 console=tty1 root=/dev/mmcblk0p2 rootfstype=ext4 rw rootwait quiet loglevel=3 net.ifnames=0
CMDLINE

    # Sync and unmount
    log_step "Syncing and unmounting..."
    sync
    umount "$WORK_DIR/mnt_boot"
    umount "$WORK_DIR/mnt_root"
    losetup -d "$loop_dev"
    loop_dev=""  # Prevent double-cleanup in trap

    # Compress (optional — keep uncompressed for dd)
    sha256sum "$img_out" > "$img_out.sha256"

    echo ""
    log_info "============================================="
    log_info "  AGNOS aarch64 SD card image created!"
    log_info "============================================="
    log_info "  Image:  $img_out"
    log_info "  Size:   $(du -h "$img_out" | cut -f1)"
    log_info "  SHA256: $(cut -d' ' -f1 < "$img_out.sha256")"
    log_info ""
    log_info "Flash to microSD with:"
    log_info "  sudo dd if=$img_out of=/dev/sdX bs=4M status=progress conv=fsync"
    log_info ""
    log_info "After boot, expand rootfs to fill card:"
    log_info "  sudo parted /dev/mmcblk0 resizepart 2 100%"
    log_info "  sudo resize2fs /dev/mmcblk0p2"
    log_info ""
    log_info "Default credentials: user/agnos  root/agnos"
    log_info "SSH: ssh user@<rpi-ip>"
    log_info "Agent runtime: http://<rpi-ip>:8090"
    log_info "LLM gateway:   http://<rpi-ip>:8088"
}

apply_edge_defaults() {
    if [[ $EDGE_MODE -eq 0 ]]; then return; fi

    # Edge images are smaller — no desktop, no GPU packages
    # But rootfs still needs ~500MB for Debian minbase + kernel + AGNOS binaries
    if [[ $IMG_SIZE_MB -eq 2048 ]]; then
        IMG_SIZE_MB=1024
    fi
    # Shrink boot partition — 128MB is plenty for kernel + DTBs + initrd
    BOOT_SIZE_MB=128
    if [[ $ISO_NAME == "agnos" ]]; then
        ISO_NAME="agnos-edge"
    fi

    # Auto-detect SY edge binary if not specified
    if [[ -z "$SY_EDGE_BINARY" ]]; then
        local search_paths=(
            "$REPO_DIR/../secureyeoman/dist/secureyeoman-edge-linux-arm64"
            "$REPO_DIR/../secureyeoman-edge-linux-arm64"
        )
        for p in "${search_paths[@]}"; do
            if [[ -f "$p" ]]; then
                SY_EDGE_BINARY="$p"
                log_info "Auto-detected SY edge binary: $SY_EDGE_BINARY"
                break
            fi
        done
    fi

    if [[ -n "$SY_EDGE_BINARY" ]] && [[ ! -f "$SY_EDGE_BINARY" ]]; then
        log_error "SY edge binary not found: $SY_EDGE_BINARY"
        log_info "Build it with: cd ../secureyeoman && CGO_ENABLED=0 GOOS=linux GOARCH=arm64 go build -ldflags '-s -w' -o dist/secureyeoman-edge-linux-arm64 ./cmd/secureyeoman-edge"
        exit 1
    fi
}

install_edge_packages() {
    if [[ $EDGE_MODE -eq 0 ]]; then return; fi

    local rootfs="${ROOTFS:-$WORK_DIR/rootfs}"

    log_step "Installing edge-specific packages..."

    # --- Install SY edge binary ---
    if [[ -n "$SY_EDGE_BINARY" ]]; then
        cp "$SY_EDGE_BINARY" "$rootfs/usr/bin/secureyeoman-edge"
        chmod 755 "$rootfs/usr/bin/secureyeoman-edge"
        log_info "  -> Installed secureyeoman-edge ($(du -h "$SY_EDGE_BINARY" | cut -f1))"
    else
        log_warn "  -> No SY edge binary provided (use --sy-edge-binary to include)"
    fi

    # --- SY edge config ---
    mkdir -p "$rootfs/etc/secureyeoman"
    cat > "$rootfs/etc/secureyeoman/edge.toml" << 'SYEDGE'
[agent]
mode = "edge"
parent_url = ""
auto_register = true

[resources]
max_memory_mb = 32
max_concurrent_tasks = 2
telemetry_interval_secs = 60
heartbeat_interval_secs = 30

[security]
sandbox = true
tls_verify = true
parent_cert_pin = ""

[tasks]
tags = []
gpu_available = false

[ota]
enabled = true
check_interval_secs = 3600
require_signature = true
rollback_on_failure = true

[telemetry]
enabled = true
local_buffer_max_mb = 4
SYEDGE

    # --- SY edge systemd unit ---
    cat > "$rootfs/etc/systemd/system/secureyeoman-edge.service" << 'SYUNIT'
[Unit]
Description=SecureYeoman Edge Agent
After=network-online.target agent-runtime.service
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/secureyeoman-edge --config /etc/secureyeoman/edge.toml
Restart=always
RestartSec=5
User=agnos
Group=agnos
WorkingDirectory=/var/lib/secureyeoman-edge
StateDirectory=secureyeoman-edge

[Install]
WantedBy=multi-user.target
SYUNIT

    # Create data directory and enable service
    mkdir -p "$rootfs/var/lib/secureyeoman-edge"

    run_chroot "
        systemctl enable secureyeoman-edge.service 2>/dev/null || true
    "

    # --- Install WireGuard (edge networking) ---
    run_chroot "
        apt-get update
        apt-get install -y --no-install-recommends wireguard-tools
        apt-get clean
        rm -rf /var/lib/apt/lists/*
    "

    log_info "  -> Edge packages installed"
}

main() {
    parse_args "$@"
    apply_edge_defaults

    local profile="Full Desktop"
    [[ $EDGE_MODE -eq 1 ]] && profile="Edge/IoT (minimal)"

    log_info "============================================="
    log_info "  AGNOS aarch64 Image Builder (RPi4/5)"
    log_info "============================================="
    log_info "  Profile:    $profile"
    log_info "  Version:    $ISO_VERSION"
    log_info "  Arch:       $ARCH"
    log_info "  Base:       Debian $DEBIAN_SUITE $DEBIAN_ARCH"
    log_info "  Image size: ${IMG_SIZE_MB} MB"
    log_info "  Output:     $OUTPUT_DIR/"
    if [[ $EDGE_MODE -eq 1 ]] && [[ -n "$SY_EDGE_BINARY" ]]; then
        log_info "  SY Edge:    $SY_EDGE_BINARY"
    fi
    log_info "============================================="
    echo ""

    check_requirements
    setup_directories
    build_userland
    create_rootfs
    install_edge_packages

    # Remove qemu-user-static from rootfs (not needed at runtime)
    rm -f "$ROOTFS/usr/bin/qemu-aarch64-static"

    # Unmount pseudo-filesystems before creating image
    umount "$ROOTFS/dev/pts" 2>/dev/null || true
    umount "$ROOTFS/sys" 2>/dev/null || true
    umount "$ROOTFS/proc" 2>/dev/null || true

    create_image

    echo ""
    log_info "Build complete!"
}

main "$@"
