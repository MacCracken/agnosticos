#!/bin/bash
# build-installer.sh — Build AGNOS x86_64 bootable installer ISO
#
# Creates a bootable live ISO from an AGNOS base rootfs + AGNOS userland.
# Supports three installation profiles: minimal, server, desktop.
# Works on bare metal, QEMU, VirtualBox, VMware.
#
# The AGNOS base rootfs is REQUIRED. Provide it via --base-rootfs or let
# the script download it from the base-rootfs-latest GitHub release.
#
# Requirements: squashfs-tools, grub, libisoburn (xorriso), mtools
# Must be run as root (or with sudo) for chroot.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/.."
WORK_DIR="$REPO_DIR/build/installer"
OUTPUT_DIR="$REPO_DIR/output"
CONFIG_DIR="$REPO_DIR/config"
USERLAND_DIR="$REPO_DIR/userland"

# Defaults
ISO_NAME="agnos"
ISO_VERSION="$(cat "${REPO_DIR}/VERSION" 2>/dev/null || echo 'dev')"
ARCH="x86_64"
PROFILE="desktop"
SKIP_BUILD=0
BASE_ROOTFS=""
SY_EDGE_BINARY=""

# GitHub release URL for AGNOS base rootfs (Tier 1 build artifact)
BASE_ROOTFS_RELEASE_URL="https://github.com/MacCracken/agnosticos/releases/download/base-rootfs-latest/agnos-base-rootfs-x86_64.tar.zst"

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
# Examples: 2026.3.17 -> 20260317, 2026.3.17-2 -> 202603172, 2026.12.5-1 -> 202612051
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
    # Zero-pad month and day to 2 digits
    printf "%s%02d%02d%s" "$y" "$m" "$d" "$patch"
}

usage() {
    cat << EOF
Usage: $0 [options]

Build AGNOS x86_64 bootable installer ISO.

Requires an AGNOS base rootfs (built by Tier 1 selfhost-build). If not provided
via --base-rootfs, the script will attempt to download it from the
base-rootfs-latest GitHub release.

Profiles:
    minimal     Headless base system: systemd, SSH, AGNOS core (daimon, hoosh, agnoshi)
    desktop     (default) Full system with Wayland compositor, Mesa, PipeWire, fonts.
                GRUB menu offers Desktop and Server boot modes from the same ISO.

Options:
    -p, --profile PROFILE   Installation profile: minimal, desktop (default: desktop)
    -n, --name NAME         ISO name prefix (default: agnos)
    -v, --version VERSION   AGNOS version (default: from VERSION file)
    -o, --output DIR        Output directory (default: output/)
    --base-rootfs PATH      AGNOS base rootfs (accepts .tar, .tar.zst, or .tar.gz)
                            If not provided, downloaded from GitHub releases automatically.
    --sy-edge-binary PATH   Include SecureYeoman edge binary (minimal profile only)
    --skip-build            Skip cargo build (use existing binaries)
    -h, --help              Show this help message

Examples:
    sudo $0                              # Desktop installer (downloads base rootfs)
    sudo $0 --profile minimal            # Headless minimal
    sudo $0 --skip-build                 # Rebuild ISO without recompiling
    sudo $0 --base-rootfs /path/to/agnos-base-rootfs.tar.zst

Output (version 2026.3.17 example):
    output/agnos-20260317-x86_64.iso              (desktop/server unified)
    output/agnos-20260317-minimal-x86_64.iso      (minimal headless)
    Patches: 2026.3.17-2 -> agnos-202603172-x86_64.iso
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -p|--profile)       PROFILE="$2"; shift 2 ;;
            -n|--name)          ISO_NAME="$2"; shift 2 ;;
            -v|--version)       ISO_VERSION="$2"; shift 2 ;;
            -o|--output)        OUTPUT_DIR="$2"; shift 2 ;;
            --base-rootfs)      BASE_ROOTFS="$2"; shift 2 ;;
            --sy-edge-binary)   SY_EDGE_BINARY="$2"; shift 2 ;;
            --skip-build)       SKIP_BUILD=1; shift ;;
            # Legacy compat
            --edge)             PROFILE="minimal"; shift ;;
            -h|--help)          usage; exit 0 ;;
            *)                  log_error "Unknown option: $1"; usage; exit 1 ;;
        esac
    done

    # Validate profile — server is accepted as alias for desktop (same ISO, different boot mode)
    case "$PROFILE" in
        minimal|desktop) ;;
        server) PROFILE="desktop" ;;
        *) log_error "Unknown profile: $PROFILE (must be minimal or desktop)"; exit 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# Profile definitions
# ---------------------------------------------------------------------------

# AGNOS binaries to install, per profile
# Returns lines of "source_name:installed_name"
profile_binaries() {
    # All profiles get core binaries
    echo "agent_runtime:agent-runtime"
    echo "llm_gateway:llm-gateway"
    echo "agnsh:agnsh"
    echo "agnos_sudo:agnos-sudo"

    # Desktop profile adds the compositor
    if [[ "$PROFILE" == "desktop" ]]; then
        echo "desktop_environment:aethersafha"
    fi
}

# Systemd units to enable, per profile
profile_enable_units() {
    # All profiles
    echo "agnos-init.service"
    echo "agent-runtime.service"
    echo "llm-gateway.service"

    # Desktop adds compositor + audio
    if [[ "$PROFILE" == "desktop" ]]; then
        echo "aethersafha.service"
        echo "agnos-pipewire.service"
        echo "agnos-wireplumber.service"
    fi
}

# Default systemd target
profile_default_target() {
    case "$PROFILE" in
        desktop) echo "agnos-graphical.target" ;;
        *)       echo "multi-user.target" ;;
    esac
}

# Whether to bundle self-hosting source tree
profile_include_sources() {
    case "$PROFILE" in
        minimal) echo 0 ;;
        *)       echo 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# Build steps
# ---------------------------------------------------------------------------

check_requirements() {
    log_step "Checking requirements..."

    local missing=()
    for cmd in mksquashfs grub-mkrescue xorriso; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        log_info "Install with: sudo pacman -S squashfs-tools grub libisoburn mtools"
        exit 1
    fi

    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (needed for chroot)"
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

    local cargo_bin
    local build_args="build --release --target x86_64-unknown-linux-musl --manifest-path $USERLAND_DIR/Cargo.toml"
    if [[ -n "${SUDO_USER:-}" ]]; then
        local user_home
        user_home=$(getent passwd "$SUDO_USER" | cut -d: -f6)
        cargo_bin="${user_home}/.cargo/bin/cargo"
        if [[ ! -x "$cargo_bin" ]]; then
            log_error "cargo not found at $cargo_bin"
            log_info "Build first, then re-run with: sudo $0 --skip-build"
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

resolve_base_rootfs() {
    # If --base-rootfs was provided, validate it
    if [[ -n "$BASE_ROOTFS" ]]; then
        if [[ ! -f "$BASE_ROOTFS" ]]; then
            log_error "Base rootfs not found: $BASE_ROOTFS"
            exit 1
        fi
        return
    fi

    # Try to download from GitHub releases
    local cached="$WORK_DIR/agnos-base-rootfs-x86_64.tar.zst"
    if [[ -f "$cached" ]]; then
        log_info "Using cached base rootfs: $cached"
        BASE_ROOTFS="$cached"
        return
    fi

    log_step "Downloading AGNOS base rootfs from GitHub releases..."
    mkdir -p "$WORK_DIR"
    if curl -fSL -o "$cached" "$BASE_ROOTFS_RELEASE_URL" 2>/dev/null; then
        log_info "  -> Downloaded base rootfs to $cached"
        BASE_ROOTFS="$cached"
    else
        log_error "No AGNOS base rootfs available."
        log_error ""
        log_error "The AGNOS base rootfs is required to build an installer ISO."
        log_error "It is built by the Tier 1 selfhost-build CI pipeline."
        log_error ""
        log_error "Options:"
        log_error "  1. Provide a local rootfs:  sudo $0 --base-rootfs /path/to/agnos-base-rootfs.tar.zst"
        log_error "  2. Build it yourself:        Run the selfhost-build workflow to create base-rootfs-latest"
        log_error "  3. Download manually from:   $BASE_ROOTFS_RELEASE_URL"
        exit 1
    fi
}

create_rootfs() {
    local rootfs="$WORK_DIR/rootfs"

    # --- Extract AGNOS base rootfs ---
    log_step "Extracting AGNOS base rootfs: $BASE_ROOTFS"
    rm -rf "$rootfs"
    mkdir -p "$rootfs"

    case "$BASE_ROOTFS" in
        *.tar.zst) zstd -d "$BASE_ROOTFS" --stdout | tar xf - -C "$rootfs" ;;
        *.tar.gz)  tar xzf "$BASE_ROOTFS" -C "$rootfs" ;;
        *.tar)     tar xf "$BASE_ROOTFS" -C "$rootfs" ;;
        *)         log_error "Unknown rootfs format: $BASE_ROOTFS (expected .tar, .tar.zst, or .tar.gz)"; exit 1 ;;
    esac

    mkdir -p "$rootfs"/{proc,sys,dev,tmp,run,var/log,boot}
    log_info "  -> AGNOS base rootfs extracted"
    log_info "  -> Packages built from source (no apt/dpkg)"

    # --- Bootstrap chroot essentials ---
    # Dynamically-linked host binaries (env, bash) won't work inside the chroot
    # unless the dynamic linker + libc are also present. Copy the minimal set.
    log_info "  Bootstrapping chroot essentials from host..."

    # Dynamic linker — required for any dynamically-linked binary
    if [[ ! -f "$rootfs/lib64/ld-linux-x86-64.so.2" ]]; then
        mkdir -p "$rootfs/lib64"
        cp /lib64/ld-linux-x86-64.so.2 "$rootfs/lib64/" 2>/dev/null || \
        cp /usr/lib64/ld-linux-x86-64.so.2 "$rootfs/lib64/" 2>/dev/null || true
    fi

    # libc — required by env, bash, and most binaries
    if [[ ! -f "$rootfs/usr/lib/libc.so.6" ]] && [[ ! -f "$rootfs/lib/libc.so.6" ]]; then
        mkdir -p "$rootfs/usr/lib"
        cp /usr/lib/libc.so.6 "$rootfs/usr/lib/" 2>/dev/null || \
        cp /lib/x86_64-linux-gnu/libc.so.6 "$rootfs/usr/lib/" 2>/dev/null || true
        # Some systems need lib -> usr/lib symlink
        if [[ ! -e "$rootfs/lib" ]]; then
            ln -sf usr/lib "$rootfs/lib" 2>/dev/null || true
        fi
    fi

    # /usr/bin/env
    if [[ ! -f "$rootfs/usr/bin/env" ]]; then
        mkdir -p "$rootfs/usr/bin"
        cp /usr/bin/env "$rootfs/usr/bin/env"
        chmod +x "$rootfs/usr/bin/env"
    fi

    # /bin/bash
    if [[ ! -f "$rootfs/bin/bash" ]]; then
        if [[ -x "$rootfs/tools/bin/bash" ]]; then
            mkdir -p "$rootfs/bin"
            ln -sf /tools/bin/bash "$rootfs/bin/bash"
        else
            mkdir -p "$rootfs/bin"
            cp /bin/bash "$rootfs/bin/bash"
            chmod +x "$rootfs/bin/bash"
            # bash needs additional libs (libtinfo/libdl/libreadline)
            for lib in libtinfo.so.6 libdl.so.2 libreadline.so.8; do
                for search in /usr/lib /lib /lib/x86_64-linux-gnu /usr/lib/x86_64-linux-gnu; do
                    if [[ -f "${search}/${lib}" ]]; then
                        cp "${search}/${lib}" "$rootfs/usr/lib/" 2>/dev/null || true
                        break
                    fi
                done
            done
        fi
    fi

    # /bin/sh
    if [[ ! -f "$rootfs/bin/sh" ]]; then
        ln -sf bash "$rootfs/bin/sh" 2>/dev/null || true
    fi

    # Verify the chroot works and define helper
    if chroot "$rootfs" /usr/bin/env true 2>/dev/null; then
        log_info "  chroot verified OK (env + bash working)"
        run_in_chroot() {
            chroot "$1" /usr/bin/env -i \
                PATH=/usr/sbin:/usr/bin:/sbin:/bin:/tools/bin \
                HOME=/root LC_ALL=POSIX \
                /bin/bash -c "$2"
        }
    else
        log_warn "  /usr/bin/env not functional in chroot — using direct bash"
        run_in_chroot() {
            chroot "$1" /bin/bash -c "
                export PATH=/usr/sbin:/usr/bin:/sbin:/bin:/tools/bin
                export HOME=/root LC_ALL=POSIX
                $2
            "
        }
    fi

    # --- Configure rootfs ---
    log_step "Configuring AGNOS rootfs..."

    # Hostname & identity
    echo "agnos" > "$rootfs/etc/hostname"
    cat > "$rootfs/etc/hosts" << 'EOF'
127.0.0.1   localhost agnos
::1         localhost agnos
EOF

    # OS release
    cat > "$rootfs/etc/os-release" << EOF
NAME="AGNOS"
VERSION="$ISO_VERSION"
ID=agnos
VERSION_ID="$ISO_VERSION"
PRETTY_NAME="AGNOS $ISO_VERSION — $PROFILE"
HOME_URL="https://github.com/agnos/agnos"
BUG_REPORT_URL="https://github.com/agnos/agnos/issues"
VARIANT="$PROFILE"
VARIANT_ID="$PROFILE"
EOF

    # Users
    run_in_chroot "$rootfs" "
        groupadd -f agnos
        useradd -r -g agnos -d /var/lib/agnos -s /usr/sbin/nologin agnos 2>/dev/null || true
        groupadd -f agnos-llm
        useradd -r -g agnos-llm -d /var/lib/agnos/models -s /usr/sbin/nologin agnos-llm 2>/dev/null || true

        useradd -m -G sudo -s /bin/bash user 2>/dev/null || true
        echo 'user:agnos' | chpasswd
        echo 'root:agnos' | chpasswd
    "

    # Desktop profile: add user to video/audio/input groups
    if [[ "$PROFILE" == "desktop" ]]; then
        run_in_chroot "$rootfs" "
            usermod -aG video,audio,input,render user 2>/dev/null || true
        "
    fi

    # AGNOS directories
    mkdir -p "$rootfs/var/lib/agnos/"{agents,models,cache,audit}
    mkdir -p "$rootfs/var/log/agnos/audit"
    mkdir -p "$rootfs/run/agnos"
    mkdir -p "$rootfs/etc/agnos"
    mkdir -p "$rootfs/usr/lib/agnos/init"

    # --- Install AGNOS binaries ---
    log_step "Installing AGNOS binaries ($PROFILE profile)..."

    local release_dir="$USERLAND_DIR/target/x86_64-unknown-linux-musl/release"
    if [[ ! -d "$release_dir" ]]; then
        release_dir="$USERLAND_DIR/target/release"
        log_warn "  -> No musl binaries found, using glibc build (may have compatibility issues)"
    fi

    while IFS=: read -r src dest; do
        if [[ -f "$release_dir/$src" ]]; then
            cp "$release_dir/$src" "$rootfs/usr/bin/$dest"
            chmod 755 "$rootfs/usr/bin/$dest"
            log_info "  -> Installed $dest ($(du -h "$release_dir/$src" | cut -f1))"
        else
            log_warn "  -> Binary not found: $src (skipping)"
        fi
    done < <(profile_binaries)

    # Symlink agnoshi -> agnsh
    ln -sf agnsh "$rootfs/usr/bin/agnoshi"

    # --- Systemd units ---
    log_step "Installing systemd units..."

    if [[ -d "$CONFIG_DIR/systemd/system" ]]; then
        # Copy all unit files (services and targets)
        cp "$CONFIG_DIR/systemd/system/"*.service "$rootfs/etc/systemd/system/" 2>/dev/null || true
        cp "$CONFIG_DIR/systemd/system/"*.target "$rootfs/etc/systemd/system/" 2>/dev/null || true

        # Enable profile-specific units
        local units_to_enable
        units_to_enable="$(profile_enable_units | tr '\n' ' ')"
        run_in_chroot "$rootfs" \
            "for unit in $units_to_enable; do systemctl enable \"\$unit\" 2>/dev/null || true; done"

        # Set default target
        local default_target
        default_target="$(profile_default_target)"
        run_in_chroot "$rootfs" "
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

    run_in_chroot "$rootfs" "
        systemctl enable systemd-networkd 2>/dev/null || true
        systemctl enable systemd-resolved 2>/dev/null || true
        systemctl enable ssh 2>/dev/null || true
        systemctl disable sshd-unix-local.socket 2>/dev/null || true
        systemctl disable sshd-vsock.socket 2>/dev/null || true
        systemctl enable serial-getty@ttyS0.service 2>/dev/null || true
    "

    # --- SSH config ---
    mkdir -p "$rootfs/etc/ssh/sshd_config.d"
    cat > "$rootfs/etc/ssh/sshd_config.d/10-agnos.conf" << 'EOF'
ListenAddress 0.0.0.0
PasswordAuthentication yes
PermitRootLogin yes
EOF

    # --- Desktop: XDG runtime dir for Wayland ---
    if [[ "$PROFILE" == "desktop" ]]; then
        mkdir -p "$rootfs/etc/tmpfiles.d"
        cat > "$rootfs/etc/tmpfiles.d/agnos-xdg.conf" << 'EOF'
d /run/user/1000 0700 user user -
EOF
        # Enable PipeWire user services
        run_in_chroot "$rootfs" "
            mkdir -p /home/user/.config/systemd/user/default.target.wants 2>/dev/null || true
            # PipeWire socket activation handled by package defaults
        "
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
        run_in_chroot "$rootfs" "
            systemctl enable secureyeoman-edge.service 2>/dev/null || true
        "
    fi

    # --- Self-hosting source tree (server and desktop profiles) ---
    if [[ "$(profile_include_sources)" == "1" ]]; then
        log_step "Bundling self-hosting source tree..."

        local src_root="$rootfs/usr/src/agnos"
        mkdir -p "$src_root"

        cp -r "$REPO_DIR/recipes" "$src_root/recipes"

        mkdir -p "$src_root/scripts"
        for script in ark-build.sh bootstrap-toolchain.sh enter-chroot.sh selfhost-validate.sh build-installer.sh build-sdcard.sh; do
            if [[ -f "$REPO_DIR/scripts/$script" ]]; then
                cp "$REPO_DIR/scripts/$script" "$src_root/scripts/"
                chmod +x "$src_root/scripts/$script"
            fi
        done

        install -m 755 "$REPO_DIR/scripts/ark-build.sh" "$rootfs/usr/bin/ark-build" 2>/dev/null || true
        install -m 755 "$REPO_DIR/scripts/selfhost-validate.sh" "$rootfs/usr/bin/selfhost-validate" 2>/dev/null || true

        cp -r "$REPO_DIR/kernel" "$src_root/kernel"
        cp -r "$REPO_DIR/userland" "$src_root/userland"
        cp "$REPO_DIR/VERSION" "$src_root/VERSION"

        if [[ -f "$USERLAND_DIR/Cargo.lock" ]]; then
            cp "$USERLAND_DIR/Cargo.lock" "$src_root/userland/Cargo.lock"
        fi

        mkdir -p "$src_root/sources"

        local src_size
        src_size=$(du -sh "$src_root" | cut -f1)
        log_info "  -> Source tree bundled at /usr/src/agnos ($src_size)"
    fi

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

AI-Native General Operating System v${ISO_VERSION}
Profile: ${profile_label}

Default credentials: user/agnos  root/agnos

EOF

    # --- Permissions ---
    run_in_chroot "$rootfs" "
        chown -R agnos:agnos /var/lib/agnos/agents 2>/dev/null || true
        chown -R agnos-llm:agnos-llm /var/lib/agnos/models 2>/dev/null || true
        chmod 750 /var/log/agnos
    "

    # --- Cleanup ---
    run_in_chroot "$rootfs" "
        rm -rf /tmp/*
    "

    log_info "  -> Rootfs configured ($PROFILE profile)"
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

    local vmlinuz=$(find "$rootfs/boot" -name 'vmlinuz-*' -type f | head -1)
    local initrd=$(find "$rootfs/boot" -name 'initrd.img-*' -type f | head -1)

    if [[ -z "$vmlinuz" ]]; then
        log_error "No kernel found in rootfs. The base rootfs may be incomplete."
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

    if [[ "$PROFILE" == "minimal" ]]; then
        # Minimal ISO — simple menu
        cat > "$WORK_DIR/iso/boot/grub/grub.cfg" << EOF
set timeout=5
set default=0

insmod all_video
insmod gfxterm

if loadfont /boot/grub/fonts/unicode.pf2 ; then
    set gfxmode=auto
    terminal_output gfxterm
fi

menuentry "AGNOS $ISO_VERSION (Minimal)" {
    linux /boot/vmlinuz boot=live toram quiet loglevel=3 net.ifnames=0 systemd.unit=multi-user.target
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Serial Console)" {
    linux /boot/vmlinuz boot=live toram console=ttyS0,115200n8 console=tty0 loglevel=5 net.ifnames=0 systemd.unit=multi-user.target
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Recovery Shell)" {
    linux /boot/vmlinuz boot=live single init=/bin/bash net.ifnames=0
    initrd /boot/initrd.img
}
EOF
    else
        # Desktop ISO — unified menu with Desktop and Server boot modes
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

menuentry "AGNOS $ISO_VERSION (Desktop)" {
    linux /boot/vmlinuz boot=live quiet loglevel=3 net.ifnames=0 systemd.unit=agnos-graphical.target
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Server — no GUI)" {
    linux /boot/vmlinuz boot=live quiet loglevel=3 net.ifnames=0 systemd.unit=multi-user.target
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Desktop — toram)" {
    linux /boot/vmlinuz boot=live toram quiet loglevel=3 net.ifnames=0 systemd.unit=agnos-graphical.target
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Debug)" {
    linux /boot/vmlinuz boot=live debug loglevel=7 systemd.log_level=debug net.ifnames=0
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Serial Console)" {
    linux /boot/vmlinuz boot=live console=ttyS0,115200n8 console=tty0 loglevel=5 net.ifnames=0 systemd.unit=multi-user.target
    initrd /boot/initrd.img
}

menuentry "AGNOS $ISO_VERSION (Recovery Shell)" {
    linux /boot/vmlinuz boot=live single init=/bin/bash net.ifnames=0
    initrd /boot/initrd.img
}
EOF
    fi
}

create_iso() {
    log_step "Creating ISO image..."

    local file_version
    file_version="$(version_to_filename "$ISO_VERSION")"
    local profile_suffix=""
    [[ "$PROFILE" == "minimal" ]] && profile_suffix="-minimal"
    local iso_file="$OUTPUT_DIR/${ISO_NAME}-${file_version}${profile_suffix}-${ARCH}.iso"

    grub-mkrescue -o "$iso_file" "$WORK_DIR/iso" \
        --modules="part_gpt part_msdos fat iso9660 normal linux all_video" \
        -- -volid "AGNOS_${ISO_VERSION}" \
        || {
        log_error "grub-mkrescue failed"
        exit 1
    }

    sha256sum "$iso_file" > "$iso_file.sha256"

    log_info "ISO created: $iso_file"
    log_info "  Size: $(du -h "$iso_file" | cut -f1)"
    log_info "  SHA256: $(cat "$iso_file.sha256" | cut -d' ' -f1)"

    echo ""
    log_info "To test with QEMU:"
    log_info "  qemu-system-x86_64 -m 2G -smp 2 -enable-kvm -cdrom $iso_file -boot d"
    log_info ""
    log_info "With port forwarding (SSH on 2222, daimon on 8090, hoosh on 8088):"
    log_info "  qemu-system-x86_64 -m 2G -smp 2 -enable-kvm -cdrom $iso_file -boot d \\"
    log_info "    -nic user,hostfwd=tcp::2222-:22,hostfwd=tcp::18090-:8090,hostfwd=tcp::18088-:8088"
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

    log_info "=========================================="
    log_info "  AGNOS Installer Builder"
    log_info "=========================================="
    log_info "  Profile:  $profile_label"
    log_info "  Version:  $ISO_VERSION"
    log_info "  Arch:     $ARCH"
    log_info "  Output:   $OUTPUT_DIR/"
    log_info "=========================================="
    echo ""

    check_requirements
    setup_directories
    resolve_base_rootfs
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
