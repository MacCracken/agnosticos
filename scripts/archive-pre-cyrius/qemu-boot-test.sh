#!/usr/bin/env bash
# qemu-boot-test.sh — Boot an AGNOS ISO in QEMU and run smoke tests
#
# Validates that AGNOS boots to a functional state:
#   1. UEFI boot from ISO
#   2. Kernel loads, initramfs mounts root
#   3. Argonaut init completes
#   4. Core services start (daimon, hoosh)
#   5. Basic userspace commands work (ark, agnsh)
#   6. Optional: run self-hosting readiness checks
#
# Usage: ./qemu-boot-test.sh [OPTIONS]
#   -i, --iso PATH        Path to AGNOS ISO (default: build/agnos-*.iso)
#   -m, --memory SIZE     VM memory in MB (default: 4096)
#   -c, --cpus N          Number of vCPUs (default: 4)
#   -t, --timeout SECS    Boot timeout in seconds (default: 300)
#   -s, --selfhost        Run self-hosting validation after boot
#   -d, --disk SIZE       Create persistent disk of SIZE (e.g., 20G)
#   -v, --verbose         Verbose QEMU output
#   -h, --help            Show this help
#
# Exit codes:
#   0 — All tests passed
#   1 — Boot failure
#   2 — Service failure
#   3 — Smoke test failure
#   4 — Self-hosting validation failure

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Colors ---
if [ -t 1 ]; then
    RED='\033[31m'; GREEN='\033[32m'; YELLOW='\033[33m'; BLUE='\033[36m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BLUE=''; NC=''
fi

log()  { echo -e "${BLUE}[qemu-test]${NC} $*"; }
ok()   { echo -e "${GREEN}[qemu-test]${NC} $*"; }
warn() { echo -e "${YELLOW}[qemu-test]${NC} $*"; }
err()  { echo -e "${RED}[qemu-test]${NC} $*" >&2; }
die()  { err "$*"; exit 1; }

# --- Defaults ---
ISO_PATH=""
VM_MEMORY=4096
VM_CPUS=4
BOOT_TIMEOUT=300
RUN_SELFHOST=false
DISK_SIZE=""
VERBOSE=false
WORK_DIR="${REPO_ROOT}/build/qemu-test"
SERIAL_LOG="${WORK_DIR}/serial.log"
MONITOR_SOCK="${WORK_DIR}/monitor.sock"
GUEST_SSH_PORT=2222
QEMU_PID=""

# --- Argument Parsing ---
usage() {
    sed -n '2,/^$/{ s/^# //; s/^#//; p }' "$0"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -i|--iso)     ISO_PATH="$2"; shift 2 ;;
            -m|--memory)  VM_MEMORY="$2"; shift 2 ;;
            -c|--cpus)    VM_CPUS="$2"; shift 2 ;;
            -t|--timeout) BOOT_TIMEOUT="$2"; shift 2 ;;
            -s|--selfhost) RUN_SELFHOST=true; shift ;;
            -d|--disk)    DISK_SIZE="$2"; shift 2 ;;
            -v|--verbose) VERBOSE=true; shift ;;
            -h|--help)    usage; exit 0 ;;
            *)            die "Unknown option: $1" ;;
        esac
    done
}

# --- Prerequisites ---
check_prerequisites() {
    log "Checking prerequisites..."

    if ! command -v qemu-system-x86_64 &>/dev/null; then
        die "qemu-system-x86_64 not found. Install: apt install qemu-system-x86"
    fi

    if ! command -v qemu-img &>/dev/null; then
        die "qemu-img not found. Install: apt install qemu-utils"
    fi

    # Find OVMF firmware for UEFI boot
    OVMF_CODE=""
    for path in \
        /usr/share/OVMF/OVMF_CODE.fd \
        /usr/share/edk2/ovmf/OVMF_CODE.fd \
        /usr/share/qemu/OVMF_CODE.fd \
        /usr/share/edk2-ovmf/x64/OVMF_CODE.fd; do
        if [ -f "$path" ]; then
            OVMF_CODE="$path"
            break
        fi
    done

    if [ -z "$OVMF_CODE" ]; then
        warn "OVMF firmware not found — falling back to BIOS boot"
        warn "Install ovmf package for UEFI testing"
    fi

    # Auto-detect ISO if not specified
    if [ -z "$ISO_PATH" ]; then
        ISO_PATH=$(find "${REPO_ROOT}/build" -maxdepth 1 -name "agnos-*.iso" -type f 2>/dev/null | sort -V | tail -1)
        if [ -z "$ISO_PATH" ]; then
            die "No ISO found. Build one first: make iso"
        fi
    fi

    if [ ! -f "$ISO_PATH" ]; then
        die "ISO not found: $ISO_PATH"
    fi

    log "ISO: $ISO_PATH"
    log "Memory: ${VM_MEMORY}MB, CPUs: ${VM_CPUS}"
    ok "Prerequisites satisfied"
}

# --- Setup ---
setup_environment() {
    log "Setting up test environment..."

    rm -rf "$WORK_DIR"
    mkdir -p "$WORK_DIR"

    # Create persistent disk if requested
    if [ -n "$DISK_SIZE" ]; then
        log "Creating ${DISK_SIZE} persistent disk..."
        qemu-img create -f qcow2 "${WORK_DIR}/disk.qcow2" "$DISK_SIZE"
    fi

    # Create cloud-init / test script seed ISO
    # This injects our smoke test scripts into the VM
    create_test_payload

    ok "Environment ready"
}

# Create an ISO with test scripts that auto-run on boot
create_test_payload() {
    local payload_dir="${WORK_DIR}/payload"
    mkdir -p "$payload_dir"

    # Smoke test script that runs inside the VM
    cat > "$payload_dir/smoke-test.sh" << 'SMOKE'
#!/bin/sh
# AGNOS boot smoke tests — runs inside the VM
# Results written to /tmp/smoke-results.txt

RESULTS="/tmp/smoke-results.txt"
PASSED=0
FAILED=0

check() {
    local name="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        echo "PASS: $name" >> "$RESULTS"
        PASSED=$((PASSED + 1))
    else
        echo "FAIL: $name" >> "$RESULTS"
        FAILED=$((FAILED + 1))
    fi
}

echo "=== AGNOS Smoke Tests ===" > "$RESULTS"
echo "Date: $(date -u)" >> "$RESULTS"
echo "" >> "$RESULTS"

# --- Kernel & Init ---
check "kernel_booted" test -f /proc/version
check "init_running" test -d /proc/1
check "hostname_set" test -n "$(hostname)"
check "root_mounted" mountpoint -q /
check "proc_mounted" mountpoint -q /proc
check "sys_mounted" mountpoint -q /sys
check "dev_mounted" mountpoint -q /dev
check "tmp_mounted" mountpoint -q /tmp

# --- Core Filesystem ---
check "usr_bin_exists" test -d /usr/bin
check "etc_exists" test -d /etc
check "var_log_exists" test -d /var/log
check "run_agnos_exists" test -d /run/agnos

# --- Core Commands ---
check "sh_works" sh -c "echo ok"
check "ls_works" ls /
check "cat_works" cat /proc/version
check "uname_works" uname -a
check "id_works" id

# --- AGNOS-Specific ---
check "agnos_version" test -f /etc/agnos/version
check "agnos_config_dir" test -d /etc/agnos
check "agnos_services_dir" test -d /etc/agnos/services

# --- Argonaut Init ---
check "argonaut_service_defs" test -d /etc/agnos/services
check "argonaut_boot_log" test -f /var/log/agnos/boot.log

# --- Package Manager (ark) ---
if command -v ark >/dev/null 2>&1; then
    check "ark_available" true
    check "ark_list" ark list
    check "ark_status" ark status
else
    echo "SKIP: ark not in PATH" >> "$RESULTS"
fi

# --- Agent Runtime (daimon) ---
if command -v curl >/dev/null 2>&1; then
    check "daimon_health" curl -sf http://127.0.0.1:8090/v1/health
    check "daimon_metrics" curl -sf http://127.0.0.1:8090/v1/metrics
    check "hoosh_health" curl -sf http://127.0.0.1:8088/v1/health
else
    echo "SKIP: curl not available" >> "$RESULTS"
fi

# --- AI Shell ---
if command -v agnsh >/dev/null 2>&1; then
    check "agnsh_available" true
    check "agnsh_version" agnsh --version
else
    echo "SKIP: agnsh not in PATH" >> "$RESULTS"
fi

# --- Networking ---
check "loopback_up" ip link show lo
check "resolv_conf" test -f /etc/resolv.conf

# --- Security ---
check "seccomp_available" test -f /proc/sys/kernel/seccomp/actions_avail
check "dev_urandom" test -c /dev/urandom

# --- Summary ---
echo "" >> "$RESULTS"
echo "=== Summary ===" >> "$RESULTS"
echo "Passed: $PASSED" >> "$RESULTS"
echo "Failed: $FAILED" >> "$RESULTS"

if [ "$FAILED" -eq 0 ]; then
    echo "STATUS: ALL_PASSED" >> "$RESULTS"
else
    echo "STATUS: SOME_FAILED" >> "$RESULTS"
fi

# Signal completion via serial console
echo "AGNOS_SMOKE_COMPLETE:${PASSED}:${FAILED}" > /dev/ttyS0 2>/dev/null || true
cat "$RESULTS" > /dev/ttyS0 2>/dev/null || true
SMOKE
    chmod +x "$payload_dir/smoke-test.sh"

    # Self-hosting readiness check script
    cat > "$payload_dir/selfhost-check.sh" << 'SELFHOST'
#!/bin/sh
# AGNOS self-hosting readiness check — runs inside the VM
# Verifies toolchain and build infrastructure are present

RESULTS="/tmp/selfhost-results.txt"
PASSED=0
FAILED=0

check() {
    local name="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        echo "PASS: $name" >> "$RESULTS"
        PASSED=$((PASSED + 1))
    else
        echo "FAIL: $name" >> "$RESULTS"
        FAILED=$((FAILED + 1))
    fi
}

echo "=== AGNOS Self-Hosting Readiness ===" > "$RESULTS"
echo "Date: $(date -u)" >> "$RESULTS"
echo "" >> "$RESULTS"

# --- Toolchain ---
check "gcc_installed" gcc --version
check "g++_installed" g++ --version
check "make_installed" make --version
check "binutils_ld" ld --version
check "binutils_as" as --version
check "binutils_ar" ar --version

# --- Rust Toolchain ---
check "rustc_installed" rustc --version
check "cargo_installed" cargo --version
check "rustfmt_installed" rustfmt --version
check "clippy_installed" cargo clippy --version

# --- Build Dependencies ---
check "cmake_installed" cmake --version
check "pkg_config" pkg-config --version
check "autoconf_installed" autoconf --version
check "automake_installed" automake --version
check "libtool_installed" libtool --version
check "bison_installed" bison --version
check "flex_installed" flex --version
check "m4_installed" m4 --version
check "perl_installed" perl --version
check "python3_installed" python3 --version

# --- Kernel Build ---
check "bc_installed" bc --version
check "kmod_installed" modprobe --version
check "kernel_headers" test -d /usr/include/linux
check "kernel_source_or_headers" test -d /usr/src/linux || test -d /lib/modules/$(uname -r)/build

# --- Libraries ---
check "libssl_dev" pkg-config --exists openssl
check "libz_dev" pkg-config --exists zlib
check "libcurl_dev" pkg-config --exists libcurl

# --- Build System (takumi/ark) ---
check "ark_build_script" test -x /usr/lib/agnos/ark-build.sh
check "recipe_dir" test -d /usr/share/agnos/recipes || test -d /etc/agnos/recipes

# --- Disk Space ---
AVAIL_GB=$(df -BG / | awk 'NR==2{print $4}' | tr -d 'G')
if [ "${AVAIL_GB:-0}" -ge 10 ]; then
    check "disk_space_10gb" true
else
    check "disk_space_10gb" false
fi

# --- Memory ---
TOTAL_MB=$(awk '/MemTotal/{print int($2/1024)}' /proc/meminfo)
if [ "${TOTAL_MB:-0}" -ge 2048 ]; then
    check "memory_2gb" true
else
    check "memory_2gb" false
fi

# --- Summary ---
echo "" >> "$RESULTS"
echo "=== Summary ===" >> "$RESULTS"
echo "Passed: $PASSED" >> "$RESULTS"
echo "Failed: $FAILED" >> "$RESULTS"

if [ "$FAILED" -eq 0 ]; then
    echo "STATUS: SELFHOST_READY" >> "$RESULTS"
else
    echo "STATUS: SELFHOST_NOT_READY" >> "$RESULTS"
fi

echo "AGNOS_SELFHOST_COMPLETE:${PASSED}:${FAILED}" > /dev/ttyS0 2>/dev/null || true
cat "$RESULTS" > /dev/ttyS0 2>/dev/null || true
SELFHOST
    chmod +x "$payload_dir/selfhost-check.sh"
}

# --- QEMU Launch ---
launch_qemu() {
    log "Launching QEMU..."

    local qemu_args=(
        qemu-system-x86_64
        -machine q35,accel=kvm:tcg
        -m "${VM_MEMORY}"
        -smp "${VM_CPUS}"
        -cdrom "$ISO_PATH"
        -boot d
        -serial "file:${SERIAL_LOG}"
        -monitor "unix:${MONITOR_SOCK},server,nowait"
        -net nic,model=virtio
        -net "user,hostfwd=tcp::${GUEST_SSH_PORT}-:22"
        -display none
        -no-reboot
    )

    # UEFI firmware
    if [ -n "${OVMF_CODE:-}" ]; then
        qemu_args+=(-drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}")
        log "UEFI boot via OVMF"
    else
        log "BIOS boot (legacy)"
    fi

    # Persistent disk
    if [ -f "${WORK_DIR}/disk.qcow2" ]; then
        qemu_args+=(-drive "file=${WORK_DIR}/disk.qcow2,format=qcow2,if=virtio")
    fi

    # Virtio-RNG for faster entropy
    qemu_args+=(-device virtio-rng-pci)

    if $VERBOSE; then
        log "QEMU command: ${qemu_args[*]}"
    fi

    # Launch in background
    "${qemu_args[@]}" &
    QEMU_PID=$!
    log "QEMU started (PID: $QEMU_PID)"
}

# --- Wait for Boot ---
wait_for_boot() {
    log "Waiting for boot (timeout: ${BOOT_TIMEOUT}s)..."

    local elapsed=0
    local interval=5
    local boot_detected=false

    # Touch serial log so tail works
    touch "$SERIAL_LOG"

    while [ $elapsed -lt "$BOOT_TIMEOUT" ]; do
        # Check QEMU is still running
        if ! kill -0 "$QEMU_PID" 2>/dev/null; then
            err "QEMU process died unexpectedly"
            dump_serial_log
            return 1
        fi

        # Check for login prompt or our test completion marker
        if grep -q "login:" "$SERIAL_LOG" 2>/dev/null; then
            boot_detected=true
            ok "Boot complete — login prompt detected (${elapsed}s)"
            break
        fi

        if grep -q "AGNOS_SMOKE_COMPLETE" "$SERIAL_LOG" 2>/dev/null; then
            boot_detected=true
            ok "Boot complete — smoke tests finished (${elapsed}s)"
            break
        fi

        # Check for kernel panic
        if grep -qi "kernel panic" "$SERIAL_LOG" 2>/dev/null; then
            err "Kernel panic detected!"
            dump_serial_log
            return 1
        fi

        # Check for initramfs failure
        if grep -qi "failed to mount root" "$SERIAL_LOG" 2>/dev/null; then
            err "Root filesystem mount failed!"
            dump_serial_log
            return 1
        fi

        sleep "$interval"
        elapsed=$((elapsed + interval))

        # Progress indicator
        if [ $((elapsed % 30)) -eq 0 ]; then
            log "Still waiting... (${elapsed}s)"
        fi
    done

    if ! $boot_detected; then
        err "Boot timeout after ${BOOT_TIMEOUT}s"
        dump_serial_log
        return 1
    fi

    return 0
}

# --- Parse Results ---
parse_smoke_results() {
    log "Parsing smoke test results..."

    if ! grep -q "AGNOS_SMOKE_COMPLETE" "$SERIAL_LOG" 2>/dev/null; then
        warn "Smoke test completion marker not found in serial log"
        warn "Tests may not have run — check if smoke-test.sh was executed"
        return 3
    fi

    local result_line
    result_line=$(grep "AGNOS_SMOKE_COMPLETE" "$SERIAL_LOG" | tail -1)
    local passed failed
    passed=$(echo "$result_line" | cut -d: -f2)
    failed=$(echo "$result_line" | cut -d: -f3)

    echo ""
    log "========================================="
    log "  AGNOS Boot Smoke Test Results"
    log "========================================="
    ok  "  Passed: $passed"

    if [ "${failed:-0}" -gt 0 ]; then
        err "  Failed: $failed"
    else
        ok  "  Failed: 0"
    fi

    # Show individual results
    echo ""
    log "Detailed results:"
    grep -E "^(PASS|FAIL|SKIP):" "$SERIAL_LOG" 2>/dev/null | while IFS= read -r line; do
        case "$line" in
            PASS:*) echo -e "  ${GREEN}$line${NC}" ;;
            FAIL:*) echo -e "  ${RED}$line${NC}" ;;
            SKIP:*) echo -e "  ${YELLOW}$line${NC}" ;;
        esac
    done

    echo ""

    if [ "${failed:-0}" -gt 0 ]; then
        return 3
    fi
    return 0
}

parse_selfhost_results() {
    if ! $RUN_SELFHOST; then
        return 0
    fi

    log "Parsing self-hosting readiness results..."

    if ! grep -q "AGNOS_SELFHOST_COMPLETE" "$SERIAL_LOG" 2>/dev/null; then
        warn "Self-hosting test completion marker not found"
        return 4
    fi

    local result_line
    result_line=$(grep "AGNOS_SELFHOST_COMPLETE" "$SERIAL_LOG" | tail -1)
    local passed failed
    passed=$(echo "$result_line" | cut -d: -f2)
    failed=$(echo "$result_line" | cut -d: -f3)

    echo ""
    log "========================================="
    log "  Self-Hosting Readiness Results"
    log "========================================="
    ok  "  Passed: $passed"

    if [ "${failed:-0}" -gt 0 ]; then
        err "  Failed: $failed"
    else
        ok  "  Failed: 0"
    fi

    # Show individual results
    grep -E "^(PASS|FAIL):" "$SERIAL_LOG" 2>/dev/null | grep -v "AGNOS Smoke" | while IFS= read -r line; do
        case "$line" in
            PASS:*) echo -e "  ${GREEN}$line${NC}" ;;
            FAIL:*) echo -e "  ${RED}$line${NC}" ;;
        esac
    done

    echo ""

    if [ "${failed:-0}" -gt 0 ]; then
        return 4
    fi
    return 0
}

# --- Cleanup ---
dump_serial_log() {
    if [ -f "$SERIAL_LOG" ]; then
        echo ""
        log "=== Serial Console Log (last 50 lines) ==="
        tail -50 "$SERIAL_LOG" 2>/dev/null || true
        log "=== Full log: $SERIAL_LOG ==="
    fi
}

cleanup() {
    if [ -n "${QEMU_PID:-}" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        log "Shutting down QEMU (PID: $QEMU_PID)..."
        # Try graceful shutdown via monitor
        if [ -S "$MONITOR_SOCK" ]; then
            echo "quit" | socat - "UNIX-CONNECT:${MONITOR_SOCK}" 2>/dev/null || true
            sleep 2
        fi
        # Force kill if still running
        if kill -0 "$QEMU_PID" 2>/dev/null; then
            kill "$QEMU_PID" 2>/dev/null || true
            wait "$QEMU_PID" 2>/dev/null || true
        fi
    fi
}

trap cleanup EXIT

# --- Main ---
main() {
    echo ""
    log "========================================="
    log "  AGNOS QEMU Boot Validation"
    log "========================================="
    echo ""

    parse_args "$@"
    check_prerequisites
    setup_environment
    launch_qemu

    local exit_code=0

    # Wait for boot
    if ! wait_for_boot; then
        exit_code=1
    fi

    # Parse smoke test results
    if [ $exit_code -eq 0 ]; then
        parse_smoke_results || exit_code=$?
    fi

    # Parse self-hosting results
    if [ $exit_code -eq 0 ] && $RUN_SELFHOST; then
        parse_selfhost_results || exit_code=$?
    fi

    # Final summary
    echo ""
    if [ $exit_code -eq 0 ]; then
        ok "========================================="
        ok "  ALL TESTS PASSED"
        ok "========================================="
    else
        err "========================================="
        err "  TESTS FAILED (exit code: $exit_code)"
        err "========================================="
        dump_serial_log
    fi

    exit $exit_code
}

main "$@"
