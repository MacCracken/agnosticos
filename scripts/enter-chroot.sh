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
    echo "ERROR: ${LFS} does not look like an AGNOS rootfs (missing /usr/bin)"
    exit 1
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
