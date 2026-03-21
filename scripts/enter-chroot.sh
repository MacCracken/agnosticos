#!/usr/bin/env bash
# enter-chroot.sh — Enter the AGNOS chroot environment for building.
#
# Mounts virtual filesystems (/proc, /sys, /dev, /run) and enters the
# chroot with a clean environment suitable for building packages.
#
# Usage:
#   LFS=/mnt/agnos ./enter-chroot.sh
#   LFS=/mnt/agnos ./enter-chroot.sh "ark-build.sh recipes/base/coreutils.toml"
#
# If a command is provided, it runs non-interactively and exits.
# Otherwise drops into an interactive bash shell.

set -euo pipefail

LFS="${LFS:?'Set LFS to the AGNOS root (e.g. /mnt/agnos)'}"

if [[ $EUID -ne 0 ]]; then
    echo "ERROR: Must be run as root (needed for mount/chroot)"
    exit 1
fi

if [[ ! -d "${LFS}/usr/bin" ]]; then
    mkdir -p "${LFS}/usr/bin"
fi

# ---------------------------------------------------------------------------
# Mount virtual filesystems (idempotent — skip if already mounted)
# ---------------------------------------------------------------------------

mount_vfs() {
    mountpoint -q "${LFS}/$1" 2>/dev/null && return
    echo "  Mounting $1..."
    case "$1" in
        dev)
            mount -v --bind /dev "${LFS}/dev"
            mount -vt devpts devpts -o gid=5,mode=0620 "${LFS}/dev/pts" 2>/dev/null || true
            mount -vt tmpfs devshm "${LFS}/dev/shm" 2>/dev/null || true
            ;;
        proc)   mount -vt proc proc "${LFS}/proc" ;;
        sys)    mount -vt sysfs sysfs "${LFS}/sys" ;;
        run)
            mount -vt tmpfs tmpfs "${LFS}/run"
            # Preserve resolv.conf for network access during builds
            if [[ -f /etc/resolv.conf ]]; then
                mkdir -p "${LFS}/etc"
                cp /etc/resolv.conf "${LFS}/etc/resolv.conf" 2>/dev/null || true
            fi
            ;;
    esac
}

echo "==> Mounting virtual filesystems in ${LFS}..."
mount_vfs dev
mount_vfs proc
mount_vfs sys
mount_vfs run

# ---------------------------------------------------------------------------
# Bootstrap essentials — ensure /usr/bin/env and /bin/bash exist in chroot
# ---------------------------------------------------------------------------

# /usr/bin/env is needed by countless scripts (#!/usr/bin/env bash)
# Copy from host if coreutils hasn't been built yet
if [[ ! -f "${LFS}/usr/bin/env" ]]; then
    echo "  Bootstrapping /usr/bin/env from host (coreutils not yet built)..."
    cp /usr/bin/env "${LFS}/usr/bin/env"
    chmod +x "${LFS}/usr/bin/env"
fi

# Find the best available bash and ensure /bin/bash exists
if [[ -x "${LFS}/bin/bash" ]]; then
    CHROOT_BASH="/bin/bash"
elif [[ -x "${LFS}/usr/bin/bash" ]]; then
    CHROOT_BASH="/usr/bin/bash"
    mkdir -p "${LFS}/bin"
    ln -sf /usr/bin/bash "${LFS}/bin/bash" 2>/dev/null || true
elif [[ -x "${LFS}/tools/bin/bash" ]]; then
    CHROOT_BASH="/tools/bin/bash"
    mkdir -p "${LFS}/bin" "${LFS}/usr/bin"
    ln -sf /tools/bin/bash "${LFS}/bin/bash" 2>/dev/null || true
    ln -sf /tools/bin/bash "${LFS}/usr/bin/bash" 2>/dev/null || true
else
    # Last resort: copy bash from host
    echo "  Bootstrapping /bin/bash from host (no bash found in chroot)..."
    mkdir -p "${LFS}/bin"
    cp /bin/bash "${LFS}/bin/bash"
    chmod +x "${LFS}/bin/bash"
    CHROOT_BASH="/bin/bash"
fi

echo "  Chroot bash: ${CHROOT_BASH}"

# Also ensure /bin/sh exists (many build scripts need it)
if [[ ! -f "${LFS}/bin/sh" ]]; then
    ln -sf bash "${LFS}/bin/sh" 2>/dev/null || true
fi

# ---------------------------------------------------------------------------
# Enter chroot
# ---------------------------------------------------------------------------

CHROOT_CMD="${1:-}"

if [[ -n "$CHROOT_CMD" ]]; then
    echo "==> Entering chroot (non-interactive): $CHROOT_CMD"
    chroot "${LFS}" /usr/bin/env -i \
        HOME=/root \
        TERM="$TERM" \
        PS1='(agnos chroot) \u:\w\$ ' \
        PATH=/usr/bin:/usr/sbin:/bin:/sbin:/tools/bin \
        MAKEFLAGS="-j$(nproc)" \
        LC_ALL=POSIX \
        /bin/bash -c "$CHROOT_CMD"
else
    echo "==> Entering chroot (interactive)..."
    echo "    Type 'exit' to leave the chroot."
    echo ""
    chroot "${LFS}" /usr/bin/env -i \
        HOME=/root \
        TERM="$TERM" \
        PS1='(agnos chroot) \u:\w\$ ' \
        PATH=/usr/bin:/usr/sbin:/bin:/sbin:/tools/bin \
        MAKEFLAGS="-j$(nproc)" \
        LC_ALL=POSIX \
        /bin/bash --login
fi

# ---------------------------------------------------------------------------
# Cleanup (unmount on exit)
# ---------------------------------------------------------------------------

echo "==> Unmounting virtual filesystems..."
umount -lf "${LFS}/dev/shm"  2>/dev/null || true
umount -lf "${LFS}/dev/pts"  2>/dev/null || true
umount -lf "${LFS}/dev"      2>/dev/null || true
umount -lf "${LFS}/proc"     2>/dev/null || true
umount -lf "${LFS}/sys"      2>/dev/null || true
umount -lf "${LFS}/run"      2>/dev/null || true

echo "==> Done."
