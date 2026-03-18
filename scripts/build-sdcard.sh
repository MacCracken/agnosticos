#!/bin/bash
# build-sdcard.sh — Build AGNOS aarch64 SD card image for Raspberry Pi 4/5
#
# Creates a bootable SD card image with Debian Trixie arm64 base + AGNOS userland.
# Supports three profiles: minimal, server, desktop.
#
# Output: agnos-<profile>-<version>-aarch64.img (flash to microSD with dd)
#
# Requirements:
#   debootstrap, squashfs-tools, dosfstools (mkfs.vfat), parted,
#   e2fsprogs (mkfs.ext4), qemu-user-static (for arm64 chroot on x86_64 host),
#   debian-archive-keyring
#
# Must be run as root (or with sudo) for debootstrap, losetup, mount.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/.."
WORK_DIR="$REPO_DIR/build/sdcard"
OUTPUT_DIR="$REPO_DIR/output"
CONFIG_DIR="$REPO_DIR/config"
USERLAND_DIR="$REPO_DIR/userland"

# RPi firmware — downloaded from raspberrypi/firmware GitHub releases
RPI_FIRMWARE_TAG="1.20250305"
RPI_FIRMWARE_URL="https://github.com/raspberrypi/firmware/archive/refs/tags/${RPI_FIRMWARE_TAG}.tar.gz"
RPI_FIRMWARE_DIR=""

# Defaults
IMG_NAME="agnos"
IMG_VERSION="$(cat "${REPO_DIR}/VERSION" 2>/dev/null || echo 'dev')"
ARCH="aarch64"
DEBIAN_ARCH="arm64"
DEBIAN_SUITE="trixie"
DEBIAN_MIRROR="http://deb.debian.org/debian"
PROFILE="desktop"
SKIP_BUILD=0
SKIP_DEBOOTSTRAP=0
SY_EDGE_BINARY=""

# Image sizing — set per profile
IMG_SIZE_MB=2047
BOOT_SIZE_MB=256

# Global rootfs path
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

# Convert canonical version (YYYY.M.D[-N]) to filename format (YYYYMMDDN)
# Examples: 2026.3.17 -> 20260317, 2026.3.17-2 -> 202603172
version_to_filename() {
    local ver="$1"
    local base patch=""
    if [[ "$ver" == *-* ]]; then
        base="${ver%-*}"
        patch="${ver##*-}"
    else
        base="$ver"
    fi
    local y m d
    IFS='.' read -r y m d <<< "$base"
    printf "%s%02d%02d%s" "$y" "$m" "$d" "$patch"
}

usage() {
    cat << EOF
Usage: $0 [options]

Build AGNOS aarch64 SD card image for Raspberry Pi 4/5.

Profiles:
    minimal     Headless: systemd, SSH, AGNOS core, SY edge agent (~1GB)
    desktop     (default) Full system with Wayland, Mesa, PipeWire, self-hosting (~3GB)

Options:
    -p, --profile PROFILE   Installation profile: minimal, desktop (default: desktop)
    -n, --name NAME         Image name prefix (default: agnos)
    -v, --version VERSION   AGNOS version (default: from VERSION file)
    -o, --output DIR        Output directory (default: output/)
    -s, --size MB           Image size in MB (default: auto per profile)
    -m, --mirror URL        Debian mirror
    --sy-edge-binary PATH   Include SecureYeoman edge binary (minimal profile)
    --skip-build            Skip cross-compilation (use existing binaries)
    --skip-debootstrap      Skip debootstrap (use existing rootfs)
    -h, --help              Show this help message

Output (version 2026.3.17 example):
    output/agnos-20260317-aarch64.img             (desktop/server unified)
    output/agnos-20260317-minimal-aarch64.img     (minimal headless)
    Patches: 2026.3.17-2 -> agnos-202603172-aarch64.img

Flash to microSD:
    sudo dd if=output/agnos-*.img of=/dev/sdX bs=4M status=progress conv=fsync
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -p|--profile)       PROFILE="$2"; shift 2 ;;
            -n|--name)          IMG_NAME="$2"; shift 2 ;;
            -v|--version)       IMG_VERSION="$2"; shift 2 ;;
            -o|--output)        OUTPUT_DIR="$2"; shift 2 ;;
            -s|--size)          IMG_SIZE_MB="$2"; shift 2 ;;
            -m|--mirror)        DEBIAN_MIRROR="$2"; shift 2 ;;
            --sy-edge-binary)   SY_EDGE_BINARY="$2"; shift 2 ;;
            --skip-build)       SKIP_BUILD=1; shift ;;
            --skip-debootstrap) SKIP_DEBOOTSTRAP=1; shift ;;
            # Legacy compat
            --edge)             PROFILE="minimal"; shift ;;
            -h|--help)          usage; exit 0 ;;
            *)                  log_error "Unknown option: $1"; usage; exit 1 ;;
        esac
    done

    case "$PROFILE" in
        minimal|desktop) ;;
        server) PROFILE="desktop" ;;
        *) log_error "Unknown profile: $PROFILE"; exit 1 ;;
    esac

    # Auto-size if user didn't override
    if [[ $IMG_SIZE_MB -eq 2047 ]]; then
        case "$PROFILE" in
            minimal) IMG_SIZE_MB=1024; BOOT_SIZE_MB=128 ;;
            # 2400MB keeps under GitHub's 2.49GB release asset limit
            desktop) IMG_SIZE_MB=2400; BOOT_SIZE_MB=256 ;;
        esac
    fi
}

# ---------------------------------------------------------------------------
# Profile definitions
# ---------------------------------------------------------------------------

profile_debian_packages() {
    case "$PROFILE" in
        minimal) echo "ca-certificates,curl" ;;
        desktop) echo "ca-certificates,curl,wireguard-tools" ;;
    esac
}

profile_extra_apt_packages() {
    # Packages installed via apt after debootstrap (space-separated)
    case "$PROFILE" in
        minimal) echo "" ;;
        desktop)
            echo "wayland-protocols libwayland-server0 \
                  mesa-vulkan-drivers libgl1-mesa-dri libgbm1 libdrm2 libegl1 \
                  libinput10 libinput-tools \
                  pipewire wireplumber pipewire-pulse pipewire-alsa \
                  xwayland libxkbcommon0 \
                  fonts-noto-core fonts-noto-mono \
                  dbus-user-session foot"
            ;;
    esac
}

profile_binaries() {
    echo "agent_runtime:agent-runtime"
    echo "llm_gateway:llm-gateway"
    echo "agnsh:agnsh"
    echo "agnos_sudo:agnos-sudo"
    if [[ "$PROFILE" == "desktop" ]]; then
        echo "desktop_environment:aethersafha"
    fi
}

profile_enable_units() {
    echo "agnos-init.service"
    echo "agent-runtime.service"
    echo "llm-gateway.service"
    if [[ "$PROFILE" == "desktop" ]]; then
        echo "aethersafha.service"
        echo "agnos-pipewire.service"
        echo "agnos-wireplumber.service"
    fi
}

profile_default_target() {
    case "$PROFILE" in
        desktop) echo "agnos-graphical.target" ;;
        *)       echo "multi-user.target" ;;
    esac
}

profile_gpu_mem() {
    case "$PROFILE" in
        minimal) echo 16 ;;
        desktop) echo 128 ;;
    esac
}

profile_gpu_overlay() {
    case "$PROFILE" in
        desktop) echo "dtoverlay=vc4-kms-v3d" ;;
        minimal) echo "" ;;
    esac
}

# ---------------------------------------------------------------------------
# RPi firmware
# ---------------------------------------------------------------------------

download_rpi_firmware() {
    log_step "Downloading Raspberry Pi firmware..."

    local fw_cache="$WORK_DIR/rpi-firmware-${RPI_FIRMWARE_TAG}"

    if [[ -d "$fw_cache/boot" ]]; then
        log_info "  -> Using cached firmware ($RPI_FIRMWARE_TAG)"
        RPI_FIRMWARE_DIR="$fw_cache"
        return
    fi

    mkdir -p "$fw_cache"
    local tarball="$WORK_DIR/rpi-firmware-${RPI_FIRMWARE_TAG}.tar.gz"

    if [[ ! -f "$tarball" ]]; then
        log_info "  -> Downloading from GitHub..."
        curl -fSL -o "$tarball" "$RPI_FIRMWARE_URL" || {
            log_error "Failed to download RPi firmware from $RPI_FIRMWARE_URL"
            log_info "You can manually download the firmware and place it at: $tarball"
            exit 1
        }
    fi

    log_info "  -> Extracting firmware..."
    tar xzf "$tarball" -C "$fw_cache" --strip-components=1

    RPI_FIRMWARE_DIR="$fw_cache"
    log_info "  -> RPi firmware ready ($RPI_FIRMWARE_TAG)"
}

install_rpi_firmware() {
    local boot_dir="$1"

    if [[ -z "$RPI_FIRMWARE_DIR" ]] || [[ ! -d "$RPI_FIRMWARE_DIR/boot" ]]; then
        log_error "RPi firmware not available — call download_rpi_firmware first"
        exit 1
    fi

    log_step "Installing RPi firmware to boot partition..."

    local fw_boot="$RPI_FIRMWARE_DIR/boot"

    # Core firmware blobs (required for boot)
    for f in start4.elf fixup4.dat start4cd.elf fixup4cd.dat \
             start4x.elf fixup4x.dat start4db.elf fixup4db.dat \
             bootcode.bin; do
        if [[ -f "$fw_boot/$f" ]]; then
            cp "$fw_boot/$f" "$boot_dir/"
        fi
    done

    # BCM2711 (RPi4) and BCM2712 (RPi5) device tree blobs
    for dtb in bcm2711-rpi-4-b.dtb bcm2711-rpi-400.dtb bcm2711-rpi-cm4.dtb \
               bcm2712-rpi-5-b.dtb bcm2712d0-rpi-5-b.dtb; do
        if [[ -f "$fw_boot/$dtb" ]]; then
            cp "$fw_boot/$dtb" "$boot_dir/"
        fi
    done

    # Overlays directory
    if [[ -d "$fw_boot/overlays" ]]; then
        cp -r "$fw_boot/overlays" "$boot_dir/"
    fi

    local fw_size
    fw_size=$(du -sh "$boot_dir" | cut -f1)
    log_info "  -> RPi firmware installed ($fw_size)"
}

# ---------------------------------------------------------------------------
# Build steps
# ---------------------------------------------------------------------------

check_requirements() {
    log_step "Checking requirements..."

    local missing=()
    for cmd in debootstrap parted mkfs.vfat mkfs.ext4 losetup curl; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done

    if [[ "$(uname -m)" != "aarch64" ]]; then
        if ! command -v qemu-aarch64-static &>/dev/null && \
           [[ ! -f /usr/bin/qemu-aarch64-static ]] && \
           [[ ! -f /proc/sys/fs/binfmt_misc/qemu-aarch64 ]]; then
            missing+=("qemu-user-static")
        fi
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        log_info "Install with: sudo pacman -S dosfstools parted e2fsprogs qemu-user-static qemu-user-static-binfmt debootstrap debian-archive-keyring"
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
            if [[ "$PROFILE" == "minimal" ]]; then
                log_warn "No AGNOS aarch64 binaries — minimal image will use SY edge binary only"
                mkdir -p "$USERLAND_DIR/target/aarch64-unknown-linux-gnu/release"
                return
            fi
            log_error "No aarch64 release binaries found. Run without --skip-build first."
            exit 1
        fi
        return
    fi

    log_step "Cross-compiling AGNOS userland for aarch64..."

    local build_args="build --release --target aarch64-unknown-linux-gnu --manifest-path $USERLAND_DIR/Cargo.toml"

    if [[ -n "${SUDO_USER:-}" ]]; then
        local user_home
        user_home=$(getent passwd "$SUDO_USER" | cut -d: -f6)
        local cross_bin="${user_home}/.cargo/bin/cross"
        local cargo_bin="${user_home}/.cargo/bin/cargo"

        if [[ -x "$cross_bin" ]]; then
            log_info "  -> Using cross (Docker-based cross-compilation)"
            sudo -u "$SUDO_USER" HOME="$user_home" PATH="${user_home}/.cargo/bin:${PATH}" \
                "$cross_bin" $build_args
        elif [[ -x "$cargo_bin" ]]; then
            log_info "  -> Using cargo with aarch64-unknown-linux-gnu target"
            sudo -u "$SUDO_USER" HOME="$user_home" PATH="${user_home}/.cargo/bin:${PATH}" \
                "$cargo_bin" build --release --target aarch64-unknown-linux-gnu \
                --manifest-path "$USERLAND_DIR/Cargo.toml"
        else
            log_error "Neither cross nor cargo found"
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

# Helper for chroot — handles both native aarch64 and foreign (x86 host)
run_chroot() {
    local rootfs="$ROOTFS"
    if [[ "$(uname -m)" != "aarch64" ]]; then
        chroot "$rootfs" /usr/bin/qemu-aarch64-static /bin/bash -c "$1"
    else
        chroot "$rootfs" /bin/bash -c "$1"
    fi
}

create_rootfs() {
    ROOTFS="$WORK_DIR/rootfs"
    local rootfs="$ROOTFS"

    if [[ $SKIP_DEBOOTSTRAP -eq 1 ]] && [[ -d "$rootfs/bin" ]]; then
        log_info "Skipping debootstrap (--skip-debootstrap)"
    else
        log_step "Bootstrapping Debian $DEBIAN_SUITE arm64 base system..."

        rm -rf "$rootfs"

        local base_pkgs="systemd,systemd-sysv,dbus,udev,iproute2,iputils-ping,kmod,procps,openssh-server,sudo,passwd,vim-tiny"
        local extra_csv
        extra_csv="$(profile_debian_packages)"
        if [[ -n "$extra_csv" ]]; then
            base_pkgs="${base_pkgs},${extra_csv}"
        fi

        if [[ "$(uname -m)" != "aarch64" ]]; then
            debootstrap --foreign --variant=minbase --arch="$DEBIAN_ARCH" \
                --include="$base_pkgs" \
                "$DEBIAN_SUITE" "$rootfs" "$DEBIAN_MIRROR"
            cp /usr/bin/qemu-aarch64-static "$rootfs/usr/bin/"
            chroot "$rootfs" /usr/bin/qemu-aarch64-static /bin/bash -c "/debootstrap/debootstrap --second-stage"
        else
            debootstrap --variant=minbase --arch="$DEBIAN_ARCH" \
                --include="$base_pkgs" \
                "$DEBIAN_SUITE" "$rootfs" "$DEBIAN_MIRROR"
        fi

        log_info "  -> Base system bootstrapped"
    fi

    # Mount pseudo-filesystems for chroot
    mount -t proc proc "$rootfs/proc" 2>/dev/null || true
    mount -t sysfs sysfs "$rootfs/sys" 2>/dev/null || true
    mount -t devpts devpts "$rootfs/dev/pts" -o ptmxmode=0666,newinstance 2>/dev/null || true

    cleanup_chroot() {
        local r="${ROOTFS:-}"
        if [[ -n "$r" ]]; then
            umount "$r/dev/pts" 2>/dev/null || true
            umount "$r/sys" 2>/dev/null || true
            umount "$r/proc" 2>/dev/null || true
        fi
    }
    trap 'cleanup_chroot' EXIT

    log_step "Configuring AGNOS rootfs ($PROFILE profile)..."

    # --- Install kernel ---
    log_step "Installing aarch64 kernel..."
    run_chroot "
        apt-get update
        apt-get install -y --no-install-recommends linux-image-arm64
        apt-get install -y --no-install-recommends firmware-brcm80211 2>/dev/null || true
        apt-get clean
        rm -rf /var/lib/apt/lists/*
    "

    # --- Profile-specific extra apt packages ---
    local extra_apt
    extra_apt="$(profile_extra_apt_packages)"
    if [[ -n "$extra_apt" ]]; then
        log_step "Installing $PROFILE profile packages..."
        run_chroot "
            apt-get update
            apt-get install -y --no-install-recommends $extra_apt
            apt-get clean
            rm -rf /var/lib/apt/lists/*
        "
        log_info "  -> Profile packages installed"
    fi

    # --- Hostname & identity ---
    echo "agnos" > "$rootfs/etc/hostname"
    cat > "$rootfs/etc/hosts" << 'EOF'
127.0.0.1   localhost agnos
::1         localhost agnos
EOF

    # --- OS release ---
    cat > "$rootfs/etc/os-release" << EOF
NAME="AGNOS"
VERSION="$IMG_VERSION"
ID=agnos
ID_LIKE=debian
VERSION_ID="$IMG_VERSION"
PRETTY_NAME="AGNOS $IMG_VERSION — $PROFILE [aarch64]"
HOME_URL="https://github.com/agnos/agnos"
BUG_REPORT_URL="https://github.com/agnos/agnos/issues"
VARIANT="$PROFILE"
VARIANT_ID="$PROFILE"
EOF

    # --- Users ---
    run_chroot "
        groupadd -f agnos
        useradd -r -g agnos -d /var/lib/agnos -s /usr/sbin/nologin agnos 2>/dev/null || true
        groupadd -f agnos-llm
        useradd -r -g agnos-llm -d /var/lib/agnos/models -s /usr/sbin/nologin agnos-llm 2>/dev/null || true
        useradd -m -G sudo -s /bin/bash user 2>/dev/null || true
        echo 'user:agnos' | chpasswd
        echo 'root:agnos' | chpasswd
    "

    if [[ "$PROFILE" == "desktop" ]]; then
        run_chroot "usermod -aG video,audio,input,render user 2>/dev/null || true"
    fi

    # --- AGNOS directories ---
    mkdir -p "$rootfs/var/lib/agnos/"{agents,models,cache,audit}
    mkdir -p "$rootfs/var/log/agnos/audit"
    mkdir -p "$rootfs/run/agnos"
    mkdir -p "$rootfs/etc/agnos"
    mkdir -p "$rootfs/usr/lib/agnos/init"

    # --- Install AGNOS binaries ---
    log_step "Installing AGNOS aarch64 binaries ($PROFILE profile)..."

    local release_dir="$USERLAND_DIR/target/aarch64-unknown-linux-gnu/release"

    while IFS=: read -r src dest; do
        if [[ -f "$release_dir/$src" ]]; then
            cp "$release_dir/$src" "$rootfs/usr/bin/$dest"
            chmod 755 "$rootfs/usr/bin/$dest"
            log_info "  -> Installed $dest ($(du -h "$release_dir/$src" | cut -f1))"
        else
            log_warn "  -> Binary not found: $src (skipping)"
        fi
    done < <(profile_binaries)

    ln -sf agnsh "$rootfs/usr/bin/agnoshi"

    # --- Systemd units ---
    log_step "Installing systemd units..."

    if [[ -d "$CONFIG_DIR/systemd/system" ]]; then
        cp "$CONFIG_DIR/systemd/system/"*.service "$rootfs/etc/systemd/system/" 2>/dev/null || true
        cp "$CONFIG_DIR/systemd/system/"*.target "$rootfs/etc/systemd/system/" 2>/dev/null || true

        local units_to_enable
        units_to_enable="$(profile_enable_units | tr '\n' ' ')"
        local default_target
        default_target="$(profile_default_target)"

        run_chroot "
            for unit in $units_to_enable; do
                systemctl enable \"\$unit\" 2>/dev/null || true
            done
            systemctl set-default '$default_target' 2>/dev/null || true
        "
        log_info "  -> Systemd units installed (default target: $default_target)"
    fi

    # --- Init scripts ---
    if [[ -d "$CONFIG_DIR/init" ]]; then
        cp "$CONFIG_DIR/init/"*.sh "$rootfs/usr/lib/agnos/init/" 2>/dev/null || true
        chmod +x "$rootfs/usr/lib/agnos/init/"*.sh 2>/dev/null || true
        log_info "  -> Init scripts installed"
    fi

    # --- /etc/issue (pre-login branding) ---
    if [[ -f "$CONFIG_DIR/issue" ]]; then
        cp "$CONFIG_DIR/issue" "$rootfs/etc/issue"
        log_info "  -> Login banner installed"
    fi

    # --- Sysctl hardening ---
    if [[ -f "$CONFIG_DIR/sysctl/99-agnos-hardening.conf" ]]; then
        mkdir -p "$rootfs/etc/sysctl.d"
        cp "$CONFIG_DIR/sysctl/99-agnos-hardening.conf" "$rootfs/etc/sysctl.d/"
    fi

    # --- Service configs ---
    if [[ -d "$CONFIG_DIR/services" ]]; then
        cp "$CONFIG_DIR/services/"*.toml "$rootfs/etc/agnos/" 2>/dev/null || true
    fi

    # --- Networking ---
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

    # --- Desktop: XDG runtime dir ---
    if [[ "$PROFILE" == "desktop" ]]; then
        mkdir -p "$rootfs/etc/tmpfiles.d"
        cat > "$rootfs/etc/tmpfiles.d/agnos-xdg.conf" << 'EOF'
d /run/user/1000 0700 user user -
EOF
        log_info "  -> Desktop environment configured"
    fi

    # --- SecureYeoman edge agent (minimal profile) ---
    if [[ "$PROFILE" == "minimal" ]] && [[ -n "$SY_EDGE_BINARY" ]] && [[ -f "$SY_EDGE_BINARY" ]]; then
        log_step "Installing SecureYeoman edge agent..."
        cp "$SY_EDGE_BINARY" "$rootfs/usr/bin/secureyeoman-edge"
        chmod 755 "$rootfs/usr/bin/secureyeoman-edge"
        log_info "  -> Installed secureyeoman-edge ($(du -h "$SY_EDGE_BINARY" | cut -f1))"

        mkdir -p "$rootfs/etc/secureyeoman"
        cat > "$rootfs/etc/secureyeoman/edge.toml" << 'SYEDGE'
[agent]
mode = "edge"
parent_url = ""
auto_register = true

[resources]
max_memory_mb = 32
max_concurrent_tasks = 2

[security]
sandbox = true
tls_verify = true

[ota]
enabled = true
check_interval_secs = 3600
rollback_on_failure = true
SYEDGE

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

        mkdir -p "$rootfs/var/lib/secureyeoman-edge"
        run_chroot "systemctl enable secureyeoman-edge.service 2>/dev/null || true"
    fi

    # --- Self-hosting source tree (server and desktop) ---
    if [[ "$PROFILE" != "minimal" ]]; then
        log_step "Bundling self-hosting source tree..."

        local src_root="$rootfs/usr/src/agnos"
        mkdir -p "$src_root"

        cp -r "$REPO_DIR/recipes" "$src_root/recipes"
        mkdir -p "$src_root/scripts"
        for script in ark-build.sh bootstrap-toolchain.sh selfhost-validate.sh build-installer.sh build-sdcard.sh; do
            [[ -f "$REPO_DIR/scripts/$script" ]] && cp "$REPO_DIR/scripts/$script" "$src_root/scripts/" && chmod +x "$src_root/scripts/$script"
        done
        install -m 755 "$REPO_DIR/scripts/ark-build.sh" "$rootfs/usr/bin/ark-build" 2>/dev/null || true
        cp -r "$REPO_DIR/kernel" "$src_root/kernel"
        cp -r "$REPO_DIR/userland" "$src_root/userland"
        cp "$REPO_DIR/VERSION" "$src_root/VERSION"
        [[ -f "$USERLAND_DIR/Cargo.lock" ]] && cp "$USERLAND_DIR/Cargo.lock" "$src_root/userland/Cargo.lock"
        mkdir -p "$src_root/sources"

        local src_size
        src_size=$(du -sh "$src_root" | cut -f1)
        log_info "  -> Source tree bundled ($src_size)"
    fi

    # --- fstab ---
    cat > "$rootfs/etc/fstab" << 'EOF'
# AGNOS aarch64 fstab
/dev/mmcblk0p2      /           ext4    defaults,noatime    0      1
/dev/mmcblk0p1      /boot       vfat    defaults            0      2
tmpfs               /tmp        tmpfs   defaults,nosuid     0      0
tmpfs               /run/agnos  tmpfs   defaults,size=64M   0      0
EOF

    # --- MOTD ---
    local profile_label
    case "$PROFILE" in
        minimal) profile_label="Minimal" ;;
        server)  profile_label="Server" ;;
        desktop) profile_label="Desktop" ;;
    esac

    cat > "$rootfs/etc/motd" << EOF

    _                         _   _       ___  ____
   / \\   __ _ _ __   ___  ___| |_(_) ___ / _ \\/ ___|
  / _ \\ / _\` | '_ \\ / _ \\/ __| __| |/ __| | | \\___ \\
 / ___ \\ (_| | | | | (_) \\__ \\ |_| | (__| |_| |___) |
/_/   \\_\\__, |_| |_|\\___/|___/\\__|_|\\___|\\___|____/
        |___/

AI-Native General Operating System v${IMG_VERSION} [aarch64]
Profile: ${profile_label}
Platform: Raspberry Pi 4/5

Default credentials: user/agnos  root/agnos

EOF

    # --- Edge rootfs minimization (minimal profile) ---
    if [[ "$PROFILE" == "minimal" ]]; then
        log_step "Minimizing rootfs for minimal deployment..."
        run_chroot "
            rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/*
            rm -rf /usr/share/locale/* /usr/share/i18n/*
            rm -rf /usr/include/* /usr/src/* /usr/games
            find / -name __pycache__ -type d -exec rm -rf {} + 2>/dev/null || true
            find / -name '*.pyc' -delete 2>/dev/null || true
            find /usr/lib -name '*.so*' -exec strip --strip-unneeded {} 2>/dev/null \\; || true
        "
        log_info "  -> Rootfs minimized"
    fi

    # --- Permissions ---
    run_chroot "
        chown -R agnos:agnos /var/lib/agnos/agents 2>/dev/null || true
        chown -R agnos-llm:agnos-llm /var/lib/agnos/models 2>/dev/null || true
        chmod 750 /var/log/agnos
    "

    # --- Cleanup ---
    run_chroot "
        apt-get clean
        rm -rf /var/lib/apt/lists/* /tmp/*
    "

    log_info "  -> Rootfs configured ($PROFILE profile)"
}

create_image() {
    log_step "Creating ${IMG_SIZE_MB}MB SD card image..."

    local file_version
    file_version="$(version_to_filename "$IMG_VERSION")"
    local profile_suffix=""
    [[ "$PROFILE" == "minimal" ]] && profile_suffix="-minimal"
    local img_out="$OUTPUT_DIR/${IMG_NAME}-${file_version}${profile_suffix}-${ARCH}.img"
    local rootfs="$WORK_DIR/rootfs"
    local loop_dev=""

    dd if=/dev/zero of="$img_out" bs=1M count="$IMG_SIZE_MB" status=none

    parted -s "$img_out" \
        mklabel msdos \
        mkpart primary fat32 1MiB "${BOOT_SIZE_MB}MiB" \
        mkpart primary ext4 "${BOOT_SIZE_MB}MiB" 100% \
        set 1 boot on

    loop_dev="$(losetup --show -fP "$img_out" 2>/dev/null)" || {
        log_error "losetup failed — are you running as root?"
        rm -f "$img_out"
        exit 1
    }

    cleanup_loop() {
        if [[ -n "${loop_dev:-}" ]]; then
            umount "$WORK_DIR/mnt_boot" 2>/dev/null || true
            umount "$WORK_DIR/mnt_root" 2>/dev/null || true
            losetup -d "$loop_dev" 2>/dev/null || true
        fi
    }
    trap cleanup_loop EXIT

    mkfs.vfat -F 32 -n "AGNOS-BOOT" "${loop_dev}p1"
    mkfs.ext4 -q -L "AGNOS-ROOT" "${loop_dev}p2"

    mkdir -p "$WORK_DIR/mnt_boot" "$WORK_DIR/mnt_root"
    mount "${loop_dev}p1" "$WORK_DIR/mnt_boot"
    mount "${loop_dev}p2" "$WORK_DIR/mnt_root"

    # --- Populate rootfs ---
    log_step "Copying rootfs to image..."
    cp -a "$rootfs/"* "$WORK_DIR/mnt_root/"
    mkdir -p "$WORK_DIR/mnt_root/boot"

    # --- Populate boot partition ---
    log_step "Setting up RPi boot partition..."

    # Install RPi firmware blobs (start4.elf, fixup4.dat, DTBs, overlays)
    install_rpi_firmware "$WORK_DIR/mnt_boot"

    # Copy kernel and initrd
    local vmlinuz=$(find "$rootfs/boot" -name 'vmlinuz-*' -type f | head -1)
    local initrd=$(find "$rootfs/boot" -name 'initrd.img-*' -type f | head -1)
    local kver=""

    if [[ -n "$vmlinuz" ]]; then
        kver=$(basename "$vmlinuz" | sed 's/vmlinuz-//')
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

    # Copy Debian DTBs (may supplement firmware DTBs for newer kernels)
    if [[ -n "$kver" ]] && [[ -d "$rootfs/usr/lib/linux-image-${kver}" ]]; then
        cp -r "$rootfs/usr/lib/linux-image-${kver}/broadcom" "$WORK_DIR/mnt_boot/" 2>/dev/null || true
        if [[ -d "$rootfs/usr/lib/linux-image-${kver}/overlays" ]]; then
            cp -r "$rootfs/usr/lib/linux-image-${kver}/overlays" "$WORK_DIR/mnt_boot/" 2>/dev/null || true
        fi
    fi

    # --- config.txt ---
    local gpu_mem
    gpu_mem="$(profile_gpu_mem)"
    local gpu_overlay
    gpu_overlay="$(profile_gpu_overlay)"

    cat > "$WORK_DIR/mnt_boot/config.txt" << RPICFG
# AGNOS — Raspberry Pi boot configuration
# Profile: $PROFILE

# 64-bit mode
arm_64bit=1

# Kernel
kernel=vmlinuz
initramfs initrd.img followkernel

# Device tree
${gpu_overlay}
dtparam=i2c_arm=on
dtparam=spi=on
dtparam=audio=$([ "$PROFILE" = "minimal" ] && echo "off" || echo "on")

# GPU memory
gpu_mem=${gpu_mem}

# Serial console
enable_uart=1

# HDMI
hdmi_force_hotplug=1

# Disable splash / rainbow screen
disable_splash=1

# Boot delay
boot_delay=1
RPICFG

    # --- cmdline.txt ---
    cat > "$WORK_DIR/mnt_boot/cmdline.txt" << CMDLINE
console=serial0,115200 console=tty1 root=/dev/mmcblk0p2 rootfstype=ext4 rw rootwait quiet loglevel=3 net.ifnames=0
CMDLINE

    # Sync and unmount
    log_step "Syncing and unmounting..."
    sync
    umount "$WORK_DIR/mnt_boot"
    umount "$WORK_DIR/mnt_root"
    losetup -d "$loop_dev"
    loop_dev=""

    sha256sum "$img_out" > "$img_out.sha256"

    echo ""
    log_info "============================================="
    log_info "  AGNOS aarch64 SD card image created!"
    log_info "============================================="
    log_info "  Profile: $PROFILE"
    log_info "  Image:   $img_out"
    log_info "  Size:    $(du -h "$img_out" | cut -f1)"
    log_info "  SHA256:  $(cut -d' ' -f1 < "$img_out.sha256")"
    log_info ""
    log_info "Flash to microSD:"
    log_info "  sudo dd if=$img_out of=/dev/sdX bs=4M status=progress conv=fsync"
    log_info ""
    log_info "After boot, expand rootfs:"
    log_info "  sudo parted /dev/mmcblk0 resizepart 2 100%"
    log_info "  sudo resize2fs /dev/mmcblk0p2"
    log_info ""
    log_info "Default credentials: user/agnos  root/agnos"
    log_info "SSH: ssh user@<rpi-ip>"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    parse_args "$@"

    local profile_label
    case "$PROFILE" in
        minimal) profile_label="Minimal (headless)" ;;
        server)  profile_label="Server" ;;
        desktop) profile_label="Desktop (Wayland)" ;;
    esac

    log_info "============================================="
    log_info "  AGNOS SD Card Builder (RPi 4/5)"
    log_info "============================================="
    log_info "  Profile:    $profile_label"
    log_info "  Version:    $IMG_VERSION"
    log_info "  Arch:       $ARCH"
    log_info "  Base:       Debian $DEBIAN_SUITE $DEBIAN_ARCH"
    log_info "  Image size: ${IMG_SIZE_MB} MB"
    log_info "  Output:     $OUTPUT_DIR/"
    log_info "============================================="
    echo ""

    check_requirements
    setup_directories
    download_rpi_firmware
    build_userland
    create_rootfs

    # Cleanup before image creation
    rm -f "$ROOTFS/usr/bin/qemu-aarch64-static"
    umount "$ROOTFS/dev/pts" 2>/dev/null || true
    umount "$ROOTFS/sys" 2>/dev/null || true
    umount "$ROOTFS/proc" 2>/dev/null || true

    create_image

    echo ""
    log_info "Build complete!"
}

main "$@"
