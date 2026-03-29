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

if [[ ! -d "${LFS}/usr" ]]; then
    mkdir -p "${LFS}/usr/bin" "${LFS}/bin" "${LFS}/lib64" "${LFS}/usr/lib"
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
# Bootstrap essentials — dynamic linker + libc + env + bash
# ---------------------------------------------------------------------------

echo "==> Bootstrapping chroot essentials..."

# Dynamic linker (architecture-aware)
ARCH=$(uname -m)
if [[ "$ARCH" == "x86_64" ]]; then
    LDSO="ld-linux-x86-64.so.2"
    LDSO_DIR="lib64"
    HOST_LIB_DIRS=("/lib64" "/usr/lib64" "/lib/x86_64-linux-gnu" "/usr/lib/x86_64-linux-gnu")
elif [[ "$ARCH" == "aarch64" ]]; then
    LDSO="ld-linux-aarch64.so.1"
    LDSO_DIR="lib"
    HOST_LIB_DIRS=("/lib" "/usr/lib" "/lib/aarch64-linux-gnu" "/usr/lib/aarch64-linux-gnu")
else
    echo "ERROR: Unsupported architecture: $ARCH"
    exit 1
fi

if [[ ! -f "${LFS}/${LDSO_DIR}/${LDSO}" ]]; then
    mkdir -p "${LFS}/${LDSO_DIR}"
    for dir in "${HOST_LIB_DIRS[@]}"; do
        if [[ -f "${dir}/${LDSO}" ]]; then
            cp "${dir}/${LDSO}" "${LFS}/${LDSO_DIR}/"
            echo "  Copied dynamic linker: ${dir}/${LDSO}"
            break
        fi
    done
    # aarch64 also needs lib64 symlink for some binaries
    if [[ "$ARCH" == "aarch64" ]] && [[ ! -e "${LFS}/lib64" ]]; then
        ln -sf lib "${LFS}/lib64" 2>/dev/null || true
    fi
fi

# libc
if [[ ! -f "${LFS}/usr/lib/libc.so.6" ]] && [[ ! -f "${LFS}/lib/libc.so.6" ]]; then
    mkdir -p "${LFS}/usr/lib"
    for dir in "${HOST_LIB_DIRS[@]}"; do
        if [[ -f "${dir}/libc.so.6" ]]; then
            cp "${dir}/libc.so.6" "${LFS}/usr/lib/"
            echo "  Copied libc from: ${dir}/libc.so.6"
            break
        fi
    done
    [[ -e "${LFS}/lib" ]] || ln -sf usr/lib "${LFS}/lib" 2>/dev/null || true
fi

# /usr/bin/env + its deps
if [[ ! -f "${LFS}/usr/bin/env" ]]; then
    mkdir -p "${LFS}/usr/bin" "${LFS}/usr/lib"
    cp /usr/bin/env "${LFS}/usr/bin/env"
    chmod +x "${LFS}/usr/bin/env"
    ldd /usr/bin/env 2>/dev/null | grep -oP '/\S+' | while read -r lib; do
        [[ -f "$lib" ]] && cp -n "$lib" "${LFS}/usr/lib/" 2>/dev/null || true
    done
fi

# /bin/bash — prefer toolchain, fall back to host
if [[ ! -f "${LFS}/bin/bash" ]]; then
    if [[ -x "${LFS}/tools/bin/bash" ]]; then
        mkdir -p "${LFS}/bin"
        ln -sf /tools/bin/bash "${LFS}/bin/bash"
    else
        mkdir -p "${LFS}/bin"
        cp /bin/bash "${LFS}/bin/bash"
        chmod +x "${LFS}/bin/bash"
    fi
fi

# /bin/sh
[[ -f "${LFS}/bin/sh" ]] || ln -sf bash "${LFS}/bin/sh" 2>/dev/null || true

# Always ensure bash + env shared library deps are present
mkdir -p "${LFS}/usr/lib"
for lib in $(ldd /bin/bash 2>/dev/null | grep -oP '/\S+'); do
    if [[ -f "$lib" ]] && [[ ! -f "${LFS}/usr/lib/$(basename "$lib")" ]]; then
        cp "$lib" "${LFS}/usr/lib/" 2>/dev/null || true
    fi
done
for lib in $(ldd /usr/bin/env 2>/dev/null | grep -oP '/\S+'); do
    if [[ -f "$lib" ]] && [[ ! -f "${LFS}/usr/lib/$(basename "$lib")" ]]; then
        cp "$lib" "${LFS}/usr/lib/" 2>/dev/null || true
    fi
done

# ---------------------------------------------------------------------------
# Determine chroot entry method
# ---------------------------------------------------------------------------

ENV_WORKS=0
if chroot "${LFS}" /usr/bin/env true 2>/dev/null; then
    ENV_WORKS=1
    echo "  chroot verified: /usr/bin/env works"
else
    echo "  /usr/bin/env not functional — using direct bash entry"
fi

# ---------------------------------------------------------------------------
# Enter chroot
# ---------------------------------------------------------------------------

CHROOT_CMD="${1:-}"
CHROOT_ENV="export HOME=/root TERM=${TERM} PATH=/usr/bin:/usr/sbin:/bin:/sbin:/tools/bin MAKEFLAGS=-j$(nproc) LC_ALL=POSIX"

if [[ -n "$CHROOT_CMD" ]]; then
    echo "==> Entering chroot (non-interactive): $CHROOT_CMD"
    if [[ "$ENV_WORKS" == "1" ]]; then
        chroot "${LFS}" /usr/bin/env -i \
            HOME=/root TERM="$TERM" \
            PS1='(agnos chroot) \u:\w\$ ' \
            PATH=/usr/bin:/usr/sbin:/bin:/sbin:/tools/bin \
            MAKEFLAGS="-j$(nproc)" LC_ALL=POSIX \
            /bin/bash -c "$CHROOT_CMD"
    else
        chroot "${LFS}" /bin/bash -c "${CHROOT_ENV}; $CHROOT_CMD"
    fi
else
    echo "==> Entering chroot (interactive)..."
    echo "    Type 'exit' to leave the chroot."
    echo ""
    if [[ "$ENV_WORKS" == "1" ]]; then
        chroot "${LFS}" /usr/bin/env -i \
            HOME=/root TERM="$TERM" \
            PS1='(agnos chroot) \u:\w\$ ' \
            PATH=/usr/bin:/usr/sbin:/bin:/sbin:/tools/bin \
            MAKEFLAGS="-j$(nproc)" LC_ALL=POSIX \
            /bin/bash --login
    else
        chroot "${LFS}" /bin/bash -c "${CHROOT_ENV}; exec /bin/bash --login"
    fi
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
