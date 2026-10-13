#!/bin/sh
# qemu-fb-smoke.sh — headless Path-C boot smoke for AGNOS.
#
# Sibling to qemu-fb-visual.sh. Same disk-image build (gnoboot EFI +
# agnos kernel in an ESP on a 64MB GPT disk), but runs QEMU without a
# graphical display, captures serial to a log, and polls the log for an
# EXPECT string. Returns 0 on match, 1 on miss / timeout.
#
# Intended use: pre-burn validation in a cycle's build gates. Verifies
# the kernel boots through gnoboot's Path-C handoff, reaches the shell
# prompt, and the new fb/serial diagnostic lines we expect for this
# cycle land in the serial transcript. NOT a substitute for iron — the
# canonical smoke targets are `AGNOS shell` (kernel up) plus whatever
# cycle-specific diagnostic line proves the bundle compiled correctly.
#
# Usage:
#   scripts/qemu-fb-smoke.sh                                # default EXPECT=AGNOS shell
#   EXPECT="fb: WC verified" scripts/qemu-fb-smoke.sh       # 1.30.11: verify PAT readback fires
#   EXPECT="fb: pf="          scripts/qemu-fb-smoke.sh      # 1.30.11: verify pf diagnostic prints
#   QEMU_TIMEOUT=120          scripts/qemu-fb-smoke.sh      # extend deadline (default 90s)
#   AGNOS_KERNEL=/path/to/agnos scripts/qemu-fb-smoke.sh    # smoke a different build
#
#   scripts/qemu-fb-smoke.sh --capture-vis                                 # screendump after EXPECT match -> /tmp/agnos-qemu-capture.png
#   scripts/qemu-fb-smoke.sh --capture-vis out.png                         # custom output path
#   CAPTURE_VIS=docs/.../qemu-1.30.12.png scripts/qemu-fb-smoke.sh         # env-driven (CI-friendly)
#   QEMU_RES=2560x1440 scripts/qemu-fb-smoke.sh --capture-vis shell.png    # capture at iron resolution
#
# --capture-vis attaches a QMP socket to the headless QEMU, waits for the
# EXPECT match plus a small settle delay, sends `screendump` via QMP, and
# converts the PPM to PNG. Only the QMP path differs from baseline; serial
# polling + EXPECT semantics are unchanged.
#
# Requires: qemu-system-x86_64, parted, mtools, edk2-ovmf.
# Requires (--capture-vis only): ffmpeg, python3.
#
# Exit codes:
#   0 — EXPECT matched in serial transcript before timeout
#       (and capture written if --capture-vis was set)
#   1 — EXPECT not matched within QEMU_TIMEOUT seconds
#   2 — toolchain / asset prerequisites missing
#   3 — EXPECT matched but capture failed (QMP / screendump / PNG convert)

set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
AGNOS_REPO="${AGNOS_REPO:-$ROOT/../agnos}"
GNOBOOT_REPO="${GNOBOOT_REPO:-$ROOT/../gnoboot}"
AGNOS_KERNEL="${AGNOS_KERNEL:-$AGNOS_REPO/build/agnos}"
GNOBOOT_EFI="${GNOBOOT_EFI:-$GNOBOOT_REPO/build/BOOTX64.EFI}"
QEMU_TIMEOUT="${QEMU_TIMEOUT:-90}"
EXPECT="${EXPECT:-AGNOS shell}"
SERIAL_LOG="${SERIAL_LOG:-/tmp/agnos-qemu-smoke-serial.log}"
QEMU_RES="${QEMU_RES:-1920x1080}"
QEMU_VGA="${QEMU_VGA:-std}"
POLL_INTERVAL="${POLL_INTERVAL:-2}"
# Visual capture (off by default). When set, smoke takes a QMP screendump
# after EXPECT matches and converts to PNG. Env override: CAPTURE_VIS=path.
# CLI: --capture-vis [path]  (path optional — defaults below).
CAPTURE_VIS="${CAPTURE_VIS:-}"
CAPTURE_SETTLE="${CAPTURE_SETTLE:-2}"  # seconds to wait after EXPECT before screendump

# CLI flag parsing (POSIX). Single flag for now; expand here if more land.
while [ $# -gt 0 ]; do
    case "$1" in
        --capture-vis)
            shift
            # Optional positional path; allow next token if it doesn't look like a flag.
            if [ $# -gt 0 ] && [ "${1#--}" = "$1" ]; then
                CAPTURE_VIS="$1"
                shift
            else
                CAPTURE_VIS="${CAPTURE_VIS:-/tmp/agnos-qemu-capture.png}"
            fi
            ;;
        -h|--help)
            sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument '$1' (try --help)" >&2
            exit 2
            ;;
    esac
done
# Env-set CAPTURE_VIS without CLI flag still needs a default if blank-but-set is meaningful;
# treat blank as "off" and a non-empty value as the output path.

# OVMF firmware probes (Arch + Ubuntu paths).
OVMF_CODE=""
OVMF_VARS_SRC=""
for c in \
    /usr/share/edk2-ovmf/x64/OVMF_CODE.4m.fd \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/qemu/OVMF.fd \
    ; do
    if [ -f "$c" ]; then OVMF_CODE="$c"; break; fi
done
for v in \
    /usr/share/edk2-ovmf/x64/OVMF_VARS.4m.fd \
    /usr/share/OVMF/OVMF_VARS.fd \
    ; do
    if [ -f "$v" ]; then OVMF_VARS_SRC="$v"; break; fi
done

# Prerequisites.
for tool in qemu-system-x86_64 parted mformat mmd mcopy; do
    command -v "$tool" >/dev/null 2>&1 || { echo "ERROR: $tool not in PATH" >&2; exit 2; }
done
[ -n "$OVMF_CODE" ]     || { echo "ERROR: OVMF_CODE.fd not found" >&2; exit 2; }
[ -n "$OVMF_VARS_SRC" ] || { echo "ERROR: OVMF_VARS.fd not found" >&2; exit 2; }
[ -f "$AGNOS_KERNEL" ]  || { echo "ERROR: $AGNOS_KERNEL not found — build agnos first" >&2; exit 2; }
[ -f "$GNOBOOT_EFI" ]   || { echo "ERROR: $GNOBOOT_EFI not found — build gnoboot first" >&2; exit 2; }

# Capture-only prereqs (avoid forcing ffmpeg/python3 on baseline smoke runs).
if [ -n "$CAPTURE_VIS" ]; then
    for tool in ffmpeg python3; do
        command -v "$tool" >/dev/null 2>&1 || { echo "ERROR: --capture-vis requires '$tool'" >&2; exit 2; }
    done
fi

# Pre-flight the serial log path (same root-owned-file footgun as qemu-fb-visual.sh).
if [ -e "$SERIAL_LOG" ] && [ ! -w "$SERIAL_LOG" ]; then
    echo "ERROR: serial log path '$SERIAL_LOG' is not writable by $(whoami)." >&2
    echo "  Fix: sudo rm '$SERIAL_LOG'  (then re-run)" >&2
    exit 2
fi
# Truncate any prior transcript so the grep below sees only this run.
: > "$SERIAL_LOG"

D=$(mktemp -d -t agnos-fb-smoke.XXXXXX)
trap 'rm -rf "$D"' EXIT
cd "$D"

echo "[1/3] Building 64MB GPT disk image with Path-C ESP layout..."
dd if=/dev/zero of=disk.img bs=1M count=64 status=none
parted -s disk.img mklabel gpt mkpart ESP fat32 1MiB 100% set 1 esp on >/dev/null 2>&1
mformat -i disk.img@@1048576 -F
mmd -i disk.img@@1048576 ::EFI
mmd -i disk.img@@1048576 ::EFI/BOOT
mmd -i disk.img@@1048576 ::boot
mcopy -i disk.img@@1048576 "$GNOBOOT_EFI" ::EFI/BOOT/BOOTX64.EFI
mcopy -i disk.img@@1048576 "$AGNOS_KERNEL" ::boot/agnos
echo "      gnoboot: $(stat -c%s "$GNOBOOT_EFI") bytes"
echo "      kernel : $(stat -c%s "$AGNOS_KERNEL") bytes"
echo "      EXPECT : '$EXPECT'"
echo "      log    : $SERIAL_LOG"

cp "$OVMF_VARS_SRC" vars.fd
chmod +w vars.fd

# VGA device args (same shape as qemu-fb-visual.sh — keeps the smoke
# path running at iron-realistic 1920x1080 unless overridden).
VGA_ARGS=""
if [ "$QEMU_RES" = "default" ] || [ "$QEMU_VGA" = "default" ]; then
    : # let qemu pick
else
    XRES="${QEMU_RES%x*}"
    YRES="${QEMU_RES#*x}"
    case "$QEMU_VGA" in
        std)    VGA_ARGS="-vga std -global VGA.xres=$XRES -global VGA.yres=$YRES" ;;
        virtio) VGA_ARGS="-vga none -device virtio-vga,xres=$XRES,yres=$YRES" ;;
        *)      echo "ERROR: QEMU_VGA='$QEMU_VGA' must be: std | virtio | default" >&2; exit 2 ;;
    esac
fi

echo "[2/3] Launching QEMU (headless, -display none) — deadline ${QEMU_TIMEOUT}s..."

# QMP socket only when --capture-vis is set; baseline smoke stays
# -monitor none. The socket lives in the run's mktemp dir and dies with it.
QMP_ARGS="-monitor none"
QMP_SOCK=""
if [ -n "$CAPTURE_VIS" ]; then
    QMP_SOCK="$D/qmp.sock"
    QMP_ARGS="-qmp unix:$QMP_SOCK,server,nowait"
    echo "      capture-vis: $CAPTURE_VIS (PNG, post-EXPECT settle=${CAPTURE_SETTLE}s)"
fi

# Background QEMU. `-display none -monitor none -serial file:` is the
# canonical headless triple — no graphical attach, no monitor I/O on
# host stdio, kernel serial flows into the log file. -no-reboot makes
# a triple-fault halt visible instead of looping. When --capture-vis is
# set, -monitor none is swapped for -qmp unix:... so the host can ask
# QEMU for a screendump after the kernel reaches EXPECT.
qemu-system-x86_64 \
    -machine q35 -m 256M -cpu max \
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
    -drive "if=pflash,format=raw,file=vars.fd" \
    -drive "file=disk.img,format=raw" \
    -device qemu-xhci,id=xhci \
    -device usb-kbd,bus=xhci.0 \
    $VGA_ARGS \
    -serial file:"$SERIAL_LOG" \
    -display none \
    $QMP_ARGS \
    -no-reboot \
    >/dev/null 2>&1 &
QEMU_PID=$!

# Capture helper: connect to QMP, send screendump, convert PPM→PNG.
# Returns 0 on success, non-zero on failure (caller decides exit-code policy).
# Inlined Python because POSIX sh has no JSON; ffmpeg handles PPM→PNG.
do_capture() {
    sock="$1"
    out_png="$2"
    ppm="$D/screendump.ppm"

    python3 - "$sock" "$ppm" <<'PY' || return 1
import json, os, socket, sys, time
sock_path, ppm_path = sys.argv[1], sys.argv[2]
s = socket.socket(socket.AF_UNIX)
s.connect(sock_path)
f = s.makefile('rwb', buffering=0)
json.loads(f.readline())                              # greeting
f.write(b'{"execute":"qmp_capabilities"}\n')
json.loads(f.readline())                              # ack
f.write(json.dumps({"execute":"screendump","arguments":{"filename":ppm_path}}).encode()+b'\n')
deadline = time.time() + 30
while time.time() < deadline:
    line = f.readline()
    if not line:
        break
    msg = json.loads(line)
    if 'return' in msg or 'error' in msg:
        break
for _ in range(30):
    if os.path.exists(ppm_path) and os.path.getsize(ppm_path) > 0:
        break
    time.sleep(0.5)
s.close()
PY

    [ -s "$ppm" ] || return 1
    ffmpeg -y -i "$ppm" "$out_png" >/dev/null 2>&1 || return 1
    [ -s "$out_png" ] || return 1
    return 0
}

# Poll for EXPECT until match or deadline. Kills QEMU on either.
DEADLINE=$(( $(date +%s) + QEMU_TIMEOUT ))
MATCHED=0
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        # QEMU exited on its own (kernel halt / triple-fault). Final grep.
        break
    fi
    if grep -qF "$EXPECT" "$SERIAL_LOG" 2>/dev/null; then
        MATCHED=1
        break
    fi
    sleep "$POLL_INTERVAL"
done

# Final grep covers the post-exit case too.
if [ "$MATCHED" = 0 ] && grep -qF "$EXPECT" "$SERIAL_LOG" 2>/dev/null; then
    MATCHED=1
fi

# Capture before tearing down QEMU (needs the VM alive for QMP).
CAPTURE_OK=0
CAPTURE_ERR=""
if [ "$MATCHED" = 1 ] && [ -n "$CAPTURE_VIS" ]; then
    # Settle delay so the post-EXPECT paint (e.g., shell prompt + cursor)
    # is on the framebuffer before screendump grabs it.
    sleep "$CAPTURE_SETTLE"
    if kill -0 "$QEMU_PID" 2>/dev/null && [ -S "$QMP_SOCK" ]; then
        if do_capture "$QMP_SOCK" "$CAPTURE_VIS"; then
            CAPTURE_OK=1
        else
            CAPTURE_ERR="screendump or PPM->PNG conversion failed"
        fi
    else
        CAPTURE_ERR="QEMU not alive or QMP socket missing at capture time"
    fi
fi

# Tear down QEMU if still alive.
if kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    # Give it a moment to terminate cleanly, then SIGKILL if still around.
    sleep 1
    kill -9 "$QEMU_PID" 2>/dev/null || true
fi
wait "$QEMU_PID" 2>/dev/null || true

echo "[3/3] QEMU exited."

if [ "$MATCHED" = 1 ]; then
    echo "PASS: EXPECT='$EXPECT' matched in serial transcript"
    echo "      ($SERIAL_LOG)"
    if [ -n "$CAPTURE_VIS" ]; then
        if [ "$CAPTURE_OK" = 1 ]; then
            echo "      capture: $CAPTURE_VIS ($(stat -c%s "$CAPTURE_VIS" 2>/dev/null || echo '?') bytes)"
            exit 0
        else
            echo "CAPTURE FAIL: $CAPTURE_ERR" >&2
            exit 3
        fi
    fi
    exit 0
else
    echo "FAIL: EXPECT='$EXPECT' not matched within ${QEMU_TIMEOUT}s"
    echo ""
    echo "Last 30 lines of serial transcript:"
    echo "--- $SERIAL_LOG ---"
    tail -30 "$SERIAL_LOG" 2>/dev/null || echo "(serial log empty)"
    echo "--- end ---"
    exit 1
fi
