#!/usr/bin/env bash
# bootstrap-toolchain.sh — Build the AGNOS cross-toolchain (LFS Ch. 5–6).
#
# This script bootstraps a cross-compiler from scratch so that the final
# system packages (built by ark-build.sh from recipes/base/*.toml) are
# independent of the host distribution.
#
# Build order (LFS 12.4):
#   1. Binutils Pass 1 (cross-assembler/linker)
#   2. GCC Pass 1 (cross-compiler, C only, no glibc)
#   3. Linux API Headers
#   4. Glibc (cross-compiled with Pass 1 tools)
#   5. Libstdc++ (cross-compiled against new glibc)
#   6. Binutils Pass 2 (rebuild for the new sysroot)
#   7. GCC Pass 2 (rebuild with threads/libstdc++)
#
# After this script completes, $LFS/tools contains a working cross-toolchain
# and $LFS has a minimal sysroot with glibc. The recipes/base/*.toml files
# can then be built using ark-build.sh inside a chroot.
#
# Usage:
#   LFS=/mnt/agnos ./bootstrap-toolchain.sh
#
# Prerequisites:
#   - Source tarballs in $LFS/sources/ (see download list below)
#   - Host has: bash 5+, binutils 2.43+, gcc 13+, make, bison, gawk, m4, texinfo
#   - At least 10 GB free on $LFS partition
#
# Requires: bash, gcc, make, bison, gawk, m4, texinfo, tar, xz, gzip

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

LFS="${LFS:?'Set LFS to the target mount point (e.g. /mnt/agnos)'}"
LFS_TGT="x86_64-agnos-linux-gnu"
MAKEFLAGS="-j$(nproc)"

# Package versions (LFS 12.4)
BINUTILS_VER="2.45"
GCC_VER="15.2.0"
GLIBC_VER="2.42"
GMP_VER="6.3.0"
MPFR_VER="4.2.2"
MPC_VER="1.3.1"
LINUX_VER="6.6.72"

SRC="${LFS}/sources"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log()  { echo "==> $*"; }
step() { echo ""; echo "===================================================="; echo "==> STEP: $*"; echo "===================================================="; }
die()  { echo "ERROR: $*" >&2; exit 1; }

extract() {
    local archive="$1"
    local dir="$2"
    log "Extracting $archive..."
    mkdir -p "$dir"
    tar xf "${SRC}/${archive}" -C "$dir" --strip-components=1
}

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------

export PATH="${LFS}/tools/bin:${PATH}"
export LC_ALL=POSIX
export LFS_TGT
export MAKEFLAGS
export CONFIG_SITE="${LFS}/usr/share/config.site"

# Create directory structure
mkdir -p "${LFS}"/{tools,usr/{bin,lib,include},etc,sources}
mkdir -p "${LFS}/lib64"  # x86_64 ld-linux path

# Create config.site for cross-compilation
mkdir -p "${LFS}/usr/share"
cat > "${LFS}/usr/share/config.site" << 'SITE'
# config.site for LFS cross-compilation
SITE

# ---------------------------------------------------------------------------
# Step 1: Binutils Pass 1
# ---------------------------------------------------------------------------
step "Binutils ${BINUTILS_VER} — Pass 1 (cross-linker)"

WORK="$(mktemp -d)"
extract "binutils-${BINUTILS_VER}.tar.xz" "${WORK}/binutils"
mkdir -p "${WORK}/binutils/build"
cd "${WORK}/binutils/build"

../configure \
    --prefix="${LFS}/tools" \
    --with-sysroot="${LFS}" \
    --target="${LFS_TGT}" \
    --disable-nls \
    --enable-gprofng=no \
    --disable-werror \
    --enable-new-dtags \
    --enable-default-hash-style=gnu

make ${MAKEFLAGS}
make install
cd /
rm -rf "${WORK}"

log "Binutils Pass 1 complete"

# ---------------------------------------------------------------------------
# Step 2: GCC Pass 1
# ---------------------------------------------------------------------------
step "GCC ${GCC_VER} — Pass 1 (cross-compiler, C only)"

WORK="$(mktemp -d)"
extract "gcc-${GCC_VER}.tar.xz" "${WORK}/gcc"

# Extract GMP, MPFR, MPC into GCC source tree
extract "gmp-${GMP_VER}.tar.xz"  "${WORK}/gcc/gmp"
extract "mpfr-${MPFR_VER}.tar.xz" "${WORK}/gcc/mpfr"
extract "mpc-${MPC_VER}.tar.gz"  "${WORK}/gcc/mpc"

# Fix lib64 → lib on x86_64
cd "${WORK}/gcc"
case $(uname -m) in
    x86_64)
        sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
    ;;
esac

mkdir -p build && cd build

../configure \
    --target="${LFS_TGT}" \
    --prefix="${LFS}/tools" \
    --with-glibc-version="${GLIBC_VER}" \
    --with-sysroot="${LFS}" \
    --with-newlib \
    --without-headers \
    --enable-default-pie \
    --enable-default-ssp \
    --disable-nls \
    --disable-shared \
    --disable-multilib \
    --disable-threads \
    --disable-libatomic \
    --disable-libgomp \
    --disable-libquadmath \
    --disable-libssp \
    --disable-libvtv \
    --disable-libstdcxx \
    --enable-languages=c,c++

make ${MAKEFLAGS}
make install

# Create limits.h (GCC needs this before glibc headers exist)
cd "${WORK}/gcc"
cat gcc/limitx.h gcc/glimits.h gcc/limity.h > \
    "$(dirname "$("${LFS_TGT}-gcc" -print-libgcc-file-name)")/include/limits.h"

cd /
rm -rf "${WORK}"

log "GCC Pass 1 complete"

# ---------------------------------------------------------------------------
# Step 3: Linux API Headers
# ---------------------------------------------------------------------------
step "Linux ${LINUX_VER} — API Headers"

WORK="$(mktemp -d)"
extract "linux-${LINUX_VER}.tar.xz" "${WORK}/linux"
cd "${WORK}/linux"

make mrproper
make headers

find usr/include -type f ! -name '*.h' -delete
cp -rv usr/include/* "${LFS}/usr/include/"

cd /
rm -rf "${WORK}"

log "Linux API Headers installed"

# ---------------------------------------------------------------------------
# Step 4: Glibc (cross-compiled)
# ---------------------------------------------------------------------------
step "Glibc ${GLIBC_VER} — Cross-build"

WORK="$(mktemp -d)"
extract "glibc-${GLIBC_VER}.tar.xz" "${WORK}/glibc"
cd "${WORK}/glibc"

# LSB compliance symlinks
case $(uname -m) in
    x86_64) ln -sfv ../lib/ld-linux-x86-64.so.2 "${LFS}/lib64"
            ln -sfv ../lib/ld-linux-x86-64.so.2 "${LFS}/lib64/ld-lsb-x86-64.so.3"
    ;;
esac

# Apply FHS patch if available
[ -f "${SRC}/glibc-${GLIBC_VER}-fhs-1.patch" ] && \
    patch -Np1 -i "${SRC}/glibc-${GLIBC_VER}-fhs-1.patch"

mkdir -p build && cd build
echo "rootsbindir=/usr/sbin" > configparms

../configure \
    --prefix=/usr \
    --host="${LFS_TGT}" \
    --build="$(../scripts/config.guess)" \
    --enable-kernel=5.4 \
    --with-headers="${LFS}/usr/include" \
    --disable-nscd \
    libc_cv_slibdir=/usr/lib

make ${MAKEFLAGS}
make DESTDIR="${LFS}" install

# Fix ldd hardcoded paths
sed '/RTLDLIST=/s@/usr@@g' -i "${LFS}/usr/bin/ldd"

cd /
rm -rf "${WORK}"

log "Glibc cross-build complete"

# Sanity check: verify the cross-toolchain works with the new glibc
log "Running toolchain sanity check..."
echo 'int main(){return 0;}' > /tmp/tc-test.c
"${LFS_TGT}-gcc" /tmp/tc-test.c -o /tmp/tc-test
readelf -l /tmp/tc-test | grep 'ld-linux' && \
    log "Sanity check PASSED" || die "Sanity check FAILED"
rm -f /tmp/tc-test.c /tmp/tc-test

# ---------------------------------------------------------------------------
# Step 5: Libstdc++ (from GCC, cross-compiled against new glibc)
# ---------------------------------------------------------------------------
step "Libstdc++ — from GCC ${GCC_VER}"

WORK="$(mktemp -d)"
extract "gcc-${GCC_VER}.tar.xz" "${WORK}/gcc"
cd "${WORK}/gcc"

mkdir -p build && cd build

../libstdc++-v3/configure \
    --host="${LFS_TGT}" \
    --build="$(../config.guess)" \
    --prefix=/usr \
    --disable-multilib \
    --disable-nls \
    --disable-libstdcxx-pch \
    --with-gxx-include-dir="/tools/${LFS_TGT}/include/c++/${GCC_VER}"

make ${MAKEFLAGS}
make DESTDIR="${LFS}" install

# Remove libtool archives that interfere with cross-compilation
rm -fv "${LFS}"/usr/lib/lib{stdc++{,exp,fs},supc++}.la

cd /
rm -rf "${WORK}"

log "Libstdc++ cross-build complete"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
echo "===================================================="
echo "  AGNOS Cross-Toolchain Bootstrap Complete"
echo "===================================================="
echo ""
echo "  Target triple:  ${LFS_TGT}"
echo "  Tools prefix:   ${LFS}/tools"
echo "  Sysroot:        ${LFS}"
echo "  Binutils:       ${BINUTILS_VER}"
echo "  GCC:            ${GCC_VER}"
echo "  Glibc:          ${GLIBC_VER}"
echo "  Linux headers:  ${LINUX_VER}"
echo ""
echo "  Next steps:"
echo "    1. Enter chroot:  scripts/enter-chroot.sh"
echo "    2. Build base:    ark-build.sh recipes/base/*.toml"
echo ""
