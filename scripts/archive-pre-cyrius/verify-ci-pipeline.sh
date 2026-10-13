#!/usr/bin/env bash
# verify-ci-pipeline.sh — Audit ALL CI/CD workflows for correctness.
# Run this BEFORE pushing any CI changes.
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0
WARN=0

pass() { echo -e "  ${GREEN}✓${NC} $1"; PASS=$((PASS + 1)); }
fail() { echo -e "  ${RED}✗${NC} $1"; FAIL=$((FAIL + 1)); }
warn() { echo -e "  ${YELLOW}⚠${NC} $1"; WARN=$((WARN + 1)); }

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WF="$REPO_ROOT/.github/workflows"

echo "============================================"
echo "  AGNOS CI/CD Pipeline Verification"
echo "============================================"
echo ""

# ── 1. Workflow files exist ──
echo "── Workflow Files ──"
for f in release.yml build-iso.yml selfhost-build.yml ci.yml publish-toolchain.yml; do
    if [[ -f "$WF/$f" ]]; then
        pass "$f exists"
    else
        fail "$f MISSING"
    fi
done

echo ""

# ── 2. Release pipeline dependency chain ──
echo "── Release Pipeline (release.yml) ──"

# Does release.yml call selfhost-build?
if grep -q 'selfhost-build.yml' "$WF/release.yml"; then
    pass "release.yml calls selfhost-build.yml"
else
    fail "release.yml does NOT call selfhost-build.yml — aarch64 rootfs will never be built"
fi

# Does build-iso depend on selfhost-build?
if grep -q "needs:.*selfhost-build" "$WF/release.yml" || grep -q "needs: \[build-release, selfhost-build\]" "$WF/release.yml"; then
    pass "build-iso waits for selfhost-build to complete"
else
    fail "build-iso does NOT wait for selfhost-build — rootfs may not exist when ISOs build"
fi

# Does create-release depend on build-iso?
if grep -q "needs:.*build-iso" "$WF/release.yml"; then
    pass "create-release waits for build-iso"
else
    fail "create-release does NOT wait for build-iso"
fi

echo ""

# ── 3. selfhost-build publishes rootfs ──
echo "── Selfhost Build (selfhost-build.yml) ──"

# x86_64 rootfs publish
if grep -q 'agnos-base-rootfs-x86_64' "$WF/selfhost-build.yml"; then
    pass "x86_64 rootfs is built and referenced"
else
    fail "x86_64 rootfs not referenced in selfhost-build"
fi

# aarch64 rootfs publish
if grep -q 'agnos-base-rootfs-aarch64' "$WF/selfhost-build.yml"; then
    pass "aarch64 rootfs is built and referenced"
else
    fail "aarch64 rootfs not referenced in selfhost-build"
fi

# Does it publish to base-rootfs-latest?
if grep -q 'base-rootfs-latest' "$WF/selfhost-build.yml"; then
    pass "publishes to base-rootfs-latest release"
else
    fail "does NOT publish to base-rootfs-latest"
fi

# Cache skip logic for x86_64
if grep -q 'x86_64.*rootfs.*cached\|cached.*x86_64\|skip.*x86_64' "$WF/selfhost-build.yml"; then
    pass "x86_64 build has cache-skip logic"
else
    warn "x86_64 build has NO cache-skip — will rebuild every time (6+ hours)"
fi

# Cache skip logic for aarch64
if grep -q 'aarch64.*rootfs.*cached\|cached.*aarch64\|skip.*aarch64' "$WF/selfhost-build.yml"; then
    pass "aarch64 build has cache-skip logic"
else
    warn "aarch64 build has NO cache-skip — will rebuild every time (6+ hours)"
fi

# LINUX_VER variable
if grep -q 'LINUX_VER:' "$WF/selfhost-build.yml"; then
    KVER=$(grep 'LINUX_VER:' "$WF/selfhost-build.yml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
    pass "LINUX_VER parameterized: $KVER"
else
    fail "Kernel version NOT parameterized — hardcoded version will break on updates"
fi

# No hardcoded kernel versions (except the LINUX_VER definition)
HARDCODED=$(grep -c '6\.6\.[0-9]*' "$WF/selfhost-build.yml" || true)
if [[ "$HARDCODED" -le 1 ]]; then
    pass "No hardcoded kernel versions (only LINUX_VER definition)"
else
    fail "Found $HARDCODED hardcoded kernel version references — should use \${LINUX_VER}"
fi

echo ""

# ── 4. build-iso checks ──
echo "── Build ISO (build-iso.yml) ──"

# All rm -rf have sudo
NOSUDO_RM=$(grep -c 'run: rm -rf' "$WF/build-iso.yml" 2>/dev/null || true)
if [[ "$NOSUDO_RM" -eq 0 ]]; then
    pass "All rm -rf commands use sudo"
else
    fail "$NOSUDO_RM rm -rf commands WITHOUT sudo — will fail on root-owned files"
fi

# aarch64 SD card downloads rootfs
if grep -q 'base-rootfs-latest.*aarch64\|agnos-base-rootfs-aarch64' "$WF/build-iso.yml"; then
    pass "aarch64 SD card job downloads rootfs from release"
else
    fail "aarch64 SD card job does NOT download rootfs"
fi

# aarch64 passes --base-rootfs to build-sdcard.sh
if grep -q 'base-rootfs.*artifacts' "$WF/build-iso.yml"; then
    pass "aarch64 passes --base-rootfs to build-sdcard.sh"
else
    fail "aarch64 does NOT pass --base-rootfs — build-sdcard.sh will try its own download"
fi

# Self-hosted runners for aarch64
AARCH_SELF=$(grep -c 'self-hosted.*arm64\|arm64.*self-hosted' "$WF/build-iso.yml" || true)
if [[ "$AARCH_SELF" -ge 2 ]]; then
    pass "aarch64 jobs use self-hosted arm64 runners ($AARCH_SELF jobs)"
else
    warn "Only $AARCH_SELF aarch64 jobs use self-hosted runners"
fi

echo ""

# ── 5. Cargo/Rust setup ──
echo "── Rust Toolchain ──"

# selfhost-build has cargo PATH setup
if grep -q 'cargo/env\|cargo --version\|rustup' "$WF/selfhost-build.yml"; then
    pass "selfhost-build has Rust toolchain setup"
else
    fail "selfhost-build missing Rust toolchain setup — cargo not in PATH"
fi

echo ""

# ── 6. Version consistency ──
echo "── Version Consistency ──"

VERSION=$(cat "$REPO_ROOT/VERSION" 2>/dev/null | tr -d '[:space:]')
CARGO_VER=$(grep '^version' "$REPO_ROOT/userland/Cargo.toml" 2>/dev/null | head -1 | sed 's/.*"\(.*\)".*/\1/')

if [[ "$VERSION" == "$CARGO_VER" ]]; then
    pass "VERSION ($VERSION) matches Cargo.toml ($CARGO_VER)"
else
    fail "VERSION ($VERSION) does NOT match Cargo.toml ($CARGO_VER)"
fi

echo ""

# ── 7. Check published rootfs assets ──
echo "── Published Release Assets ──"

if command -v curl &>/dev/null; then
    RELEASE_JSON=$(curl -sL "https://api.github.com/repos/MacCracken/agnosticos/releases/tags/base-rootfs-latest" 2>/dev/null || echo '{}')

    if echo "$RELEASE_JSON" | grep -q 'agnos-base-rootfs-x86_64'; then
        pass "x86_64 rootfs published in base-rootfs-latest"
    else
        warn "x86_64 rootfs NOT in base-rootfs-latest — selfhost-build needs to run"
    fi

    if echo "$RELEASE_JSON" | grep -q 'agnos-base-rootfs-aarch64'; then
        pass "aarch64 rootfs published in base-rootfs-latest"
    else
        fail "aarch64 rootfs NOT in base-rootfs-latest — THIS IS WHY SD CARD BUILDS FAIL"
    fi
else
    warn "curl not available — skipping remote asset check"
fi

echo ""

# ── 8. Script permissions ──
echo "── Script Permissions ──"

for script in build-installer.sh build-sdcard.sh build-edge.sh bootstrap-toolchain.sh build-selfhosting-iso.sh version-bump.sh bench-history.sh verify-ci-pipeline.sh; do
    if [[ -f "$REPO_ROOT/scripts/$script" ]]; then
        if [[ -x "$REPO_ROOT/scripts/$script" ]]; then
            pass "$script is executable"
        else
            warn "$script exists but NOT executable"
        fi
    fi
done

echo ""

# ── Summary ──
echo "============================================"
echo -e "  Results: ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC}, ${YELLOW}$WARN warnings${NC}"
echo "============================================"

if [[ "$FAIL" -gt 0 ]]; then
    echo ""
    echo -e "${RED}FIX THE FAILURES BEFORE PUSHING.${NC}"
    exit 1
fi

if [[ "$WARN" -gt 0 ]]; then
    echo ""
    echo -e "${YELLOW}Warnings should be addressed but won't block.${NC}"
fi
