#!/bin/sh
# qemu-fb-visual.sh — boot the AGNOS Path-C chain (gnoboot → kernel) in
# QEMU + OVMF with a visible GUI window for framebuffer verification.
#
# Use this to eyeball framebuffer-refresh quality, scroll cleanliness,
# edge-pixel behavior, and shell rendering on the real GOP path WITHOUT
# burning iron. Bypasses the arcade-cabinet bezel that hides edge
# pixels on archaemenid photos — every actual fb pixel is visible in
# the QEMU window.
#
# Usage:
#   scripts/qemu-fb-visual.sh                 # default gtk display
#   DISPLAY_BACKEND=sdl scripts/qemu-fb-visual.sh
#   QEMU_TIMEOUT=120 scripts/qemu-fb-visual.sh
#
# Requires: qemu-system-x86_64, parted, mtools, edk2-ovmf.
# Requires: a graphical session (DISPLAY / WAYLAND_DISPLAY set) — run
#   this from your desktop terminal, not a remote / headless context.
#
# After the window opens:
#   - Watch gnoboot's banner: "gnoboot v0.2.0: handing off to kernel"
#   - Then the kernel boot log scrolls (kprintln output mirrored to fb).
#   - Finally the agnoshi prompt: "AGNOS shell v1.30.X (type 'help')"
#                                  "agnos> "
#   - Test scroll quality: type a bunch of stuff that wraps, watch for
#     pixel-pattern noise / refresh artifacts.
#   - Test pitch padding: look at the right edge — should be clean
#     black, no firmware-paint stripe.
#   - Quit: close the window, or hit Ctrl-Alt-Q in the qemu window.
#
# Verify against the pre-change baseline by booting the previous
# build/agnos binary instead (point AGNOS_KERNEL at it).

set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
AGNOS_REPO="${AGNOS_REPO:-$ROOT/../agnos}"
GNOBOOT_REPO="${GNOBOOT_REPO:-$ROOT/../gnoboot}"
AGNOS_KERNEL="${AGNOS_KERNEL:-$AGNOS_REPO/build/agnos}"
GNOBOOT_EFI="${GNOBOOT_EFI:-$GNOBOOT_REPO/build/BOOTX64.EFI}"
DISPLAY_BACKEND="${DISPLAY_BACKEND:-gtk}"
QEMU_TIMEOUT="${QEMU_TIMEOUT:-180}"
# Serial log path. Default to /tmp so a prior sudo run can't poison the
# in-tree scripts/build/ dir (root-owned files become un-overwriteable).
# Override with SERIAL_LOG=/path for in-tree capture.
SERIAL_LOG="${SERIAL_LOG:-/tmp/agnos-qemu-fb-visual-serial.log}"
# Framebuffer resolution. Defaults to 1920x1080 to match archaemenid
# iron geometry — stresses scroll perf with realistic 2M-pixel writes
# and exercises the pitch-aware loops at iron-class extents. Override:
#   QEMU_RES=800x600   — closer to OVMF's default GOP mode
#   QEMU_RES=default   — skip device override, let QEMU/OVMF pick
QEMU_RES="${QEMU_RES:-1920x1080}"

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

# Sanity checks.
for tool in qemu-system-x86_64 parted mformat mmd mcopy; do
    command -v "$tool" >/dev/null 2>&1 || { echo "ERROR: $tool not in PATH" >&2; exit 1; }
done
[ -n "$OVMF_CODE" ]     || { echo "ERROR: OVMF_CODE.fd not found" >&2; exit 1; }
[ -n "$OVMF_VARS_SRC" ] || { echo "ERROR: OVMF_VARS.fd not found" >&2; exit 1; }
[ -f "$AGNOS_KERNEL" ]  || { echo "ERROR: $AGNOS_KERNEL not found — build agnos first" >&2; exit 1; }
[ -f "$GNOBOOT_EFI" ]   || { echo "ERROR: $GNOBOOT_EFI not found — build gnoboot first" >&2; exit 1; }

# Display-backend probe. gtk / sdl need their qemu-ui-* package installed
# (Arch splits these out from qemu-system-x86). vnc is built into the
# core qemu-system binary and always available — preferred fallback when
# the UI packages aren't installed.
DISPLAY_OK=0
if [ "$DISPLAY_BACKEND" = "vnc" ]; then
    DISPLAY_OK=1
else
    if qemu-system-x86_64 -display help 2>&1 | grep -qE "^${DISPLAY_BACKEND}\$"; then
        DISPLAY_OK=1
    fi
fi
if [ "$DISPLAY_OK" = "0" ]; then
    echo "ERROR: QEMU display backend '$DISPLAY_BACKEND' not available."
    echo ""
    echo "Available backends on this system:"
    qemu-system-x86_64 -display help 2>&1 | sed -n '2,$p' | head -10
    echo ""
    echo "Fix options:"
    echo "  1. Install gtk backend:    sudo pacman -S qemu-ui-gtk"
    echo "  2. Install sdl backend:    sudo pacman -S qemu-ui-sdl"
    echo "  3. Use VNC (no install):   DISPLAY_BACKEND=vnc $0"
    echo "                             then connect any VNC client to localhost:5901"
    exit 1
fi

# Graphical-session check (only required for window-attached backends).
if [ "$DISPLAY_BACKEND" != "vnc" ] && [ -z "$DISPLAY" ] && [ -z "$WAYLAND_DISPLAY" ]; then
    echo "ERROR: no DISPLAY / WAYLAND_DISPLAY set — run this from a desktop terminal."
    echo "  Or use DISPLAY_BACKEND=vnc $0 for a no-window VNC server."
    exit 1
fi

D=$(mktemp -d -t agnos-fb-visual.XXXXXX)
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

cp "$OVMF_VARS_SRC" vars.fd
chmod +w vars.fd

# Pre-flight the serial log path. If a stale root-owned file exists
# (left by a previous sudo run), fail fast with a clear fix hint instead
# of letting qemu emit a cryptic "permission denied".
if [ -e "$SERIAL_LOG" ] && [ ! -w "$SERIAL_LOG" ]; then
    echo "ERROR: serial log path '$SERIAL_LOG' is not writable by $(whoami)."
    echo "  Likely cause: a previous sudo run left a root-owned file."
    echo "  Fix: sudo rm '$SERIAL_LOG'  (then re-run this script)"
    echo "  Or override:  SERIAL_LOG=/path/you/own.log $0"
    exit 1
fi

# Build the -display argument. VNC needs an addr:display-number; window
# backends take the bare name. VNC mode: connect with any VNC client
# (vncviewer, Remmina, KRDC, …) to localhost:5901.
DISPLAY_ARG="$DISPLAY_BACKEND"
if [ "$DISPLAY_BACKEND" = "vnc" ]; then
    DISPLAY_ARG="vnc=127.0.0.1:1"
    echo "[2/3] Launching QEMU + OVMF with VNC display on localhost:5901..."
    echo "      Connect with any VNC client (vncviewer, Remmina, KRDC):"
    echo "        vncviewer localhost:5901"
    echo "      Or via display number form: vncviewer localhost:1"
else
    echo "[2/3] Launching QEMU + OVMF with -display $DISPLAY_BACKEND..."
fi

# Build VGA device args. Two paths:
#   QEMU_VGA=std       — standard VGA (built into qemu-system-x86, always
#                        available). Honors xres/yres via -global flags.
#                        DEFAULT — no extra package install needed.
#   QEMU_VGA=virtio    — virtio-vga (needs qemu-hw-display-virtio-vga
#                        package on Arch). xres/yres via device params.
#   QEMU_VGA=default   — let QEMU pick (cirrus VGA, low res).
#
# xres/yres set OVMF's preferred GOP mode so the kernel sees an
# iron-realistic framebuffer (default 1920x1080 matches archaemenid).
# This stresses the scroll path with ~2M pixel writes per scroll and
# surfaces any latent bugs in the pitch-aware loops at iron extents.
QEMU_VGA="${QEMU_VGA:-std}"
VGA_ARGS=""
if [ "$QEMU_RES" = "default" ] || [ "$QEMU_VGA" = "default" ]; then
    echo "      Resolution: QEMU default (cirrus VGA, typically 640x480 or 800x600)"
else
    XRES="${QEMU_RES%x*}"
    YRES="${QEMU_RES#*x}"
    if ! echo "$XRES" | grep -qE "^[0-9]+$" || ! echo "$YRES" | grep -qE "^[0-9]+$"; then
        echo "ERROR: QEMU_RES='$QEMU_RES' is not in WxH form (e.g. 1920x1080)." >&2
        exit 1
    fi
    case "$QEMU_VGA" in
        std)
            # Standard VGA with custom xres/yres via -global. bochs-display-
            # compatible; OVMF builds an EDID and registers the matching
            # GOP mode. Built into qemu-system-x86 — no extra package.
            VGA_ARGS="-vga std -global VGA.xres=$XRES -global VGA.yres=$YRES"
            echo "      Resolution: ${XRES}x${YRES} (std VGA, no extra pkg needed)"
            ;;
        virtio)
            VGA_ARGS="-vga none -device virtio-vga,xres=$XRES,yres=$YRES"
            echo "      Resolution: ${XRES}x${YRES} (virtio-vga, requires qemu-hw-display-virtio-vga)"
            ;;
        *)
            echo "ERROR: QEMU_VGA='$QEMU_VGA' must be: std | virtio | default" >&2
            exit 1
            ;;
    esac
fi
echo "      Watch for : gnoboot banner → kernel boot log → agnos> prompt"
echo "      Verify    : clean scroll, no right-edge stripe, no lower-region noise"
echo "      Quit      : close the QEMU window, Ctrl-Alt-Q in window, or disconnect VNC + Ctrl-C here"
echo ""

# -cpu max: SMEP/SMAP/RDRAND available (kernel uses RDRAND in kaslr_seed).
# -m 256M : matches gnoboot/tests/ovmf_smoke.sh default.
# -no-reboot: triple-fault halts instead of looping; visible failure.
# -device qemu-xhci + usb-kbd: gives the kernel an xHCI controller with a
#   virtual HID keyboard attached. Without this, q35 has NO USB controller
#   and the kernel's xHCI HID driver has nothing to enumerate — you can
#   see the boot but can't type into the shell. (Iron uses xHCI + a real
#   keyboard; this mirrors that path on the QEMU side.)
# Serial mirrored to a file so we have a transcript even with a window.
timeout "$QEMU_TIMEOUT" \
    qemu-system-x86_64 \
        -machine q35 -m 256M -cpu max \
        -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
        -drive "if=pflash,format=raw,file=vars.fd" \
        -drive "file=disk.img,format=raw" \
        -device qemu-xhci,id=xhci \
        -device usb-kbd,bus=xhci.0 \
        $VGA_ARGS \
        -serial file:"$SERIAL_LOG" \
        -display "$DISPLAY_ARG" \
        -no-reboot \
    || rc=$?

echo ""
echo "[3/3] QEMU exited (rc=${rc:-0})."
echo "      Serial transcript: $SERIAL_LOG"
echo ""
echo "Verification checklist (1.30.10 framebuffer scope):"
echo "  [ ] gnoboot banner rendered cleanly"
echo "  [ ] kernel boot log scrolled with clean refresh (no pixel-pattern noise)"
echo "  [ ] agnos> shell prompt visible"
echo "  [ ] right-edge column is clean black (no firmware-paint stripe)"
echo "  [ ] scroll a bench output (type 'bench') and watch for refresh artifacts"
echo "  [ ] quit via window close or Ctrl-Alt-Q"
