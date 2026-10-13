#!/bin/sh
# capture-amdgpu-modeset-clock.sh — isolate amdgpu's PIXEL-CLOCK + PHY re-lock
# sequence during a real HDMI modeset, stripped of the compositor page-flip noise
# that buried the P5a capture (amdgpu-hdmi-modeset-writes-0717.txt, 11.5k lines).
#
# WHY: agnos's ATOM interpreter is proven bit-correct on iron, but the transmitter
# ENABLE power-cycles the PHY and blanks the live pipe because agnos skips the
# clock/OTG re-commit amdgpu wraps it in. Before agnos attempts that modeset (a
# real blank risk), capture what amdgpu ACTUALLY writes to the host during a cold
# HDMI bring-up: the DCCG/PHYPLL/symclk registers (host-visible ⇒ agnos can
# replicate them directly, no ATOM guessing), and whether the transmitter/clock
# go through the DMCUB inbox (opaque ⇒ we learn that too and pivot).
#
# SAFE: read-only tracing on the Linux dev box. No agnos involvement.
#
# USAGE (as root, on archaemenid running amdgpu):
#   sudo ./scripts/capture-amdgpu-modeset-clock.sh
# then follow the prompts: UNPLUG the HDMI cable, wait ~3s, PLUG it back in.
# Output: ./amdgpu-modeset-clock.txt  (hand it to me).

set -eu
OUT="${1:-./amdgpu-modeset-clock.txt}"

T=/sys/kernel/tracing
[ -d "$T/events" ] || T=/sys/kernel/debug/tracing
[ -d "$T/events" ] || { mount -t tracefs nodev /sys/kernel/tracing 2>/dev/null && T=/sys/kernel/tracing; }
[ -w "$T/tracing_on" ] || { echo "tracefs not writable at $T (need sudo)"; exit 1; }

EV=amdgpu/amdgpu_device_wreg
[ -e "$T/events/$EV/enable" ] || {
    echo "tracepoint $EV not present. Available amdgpu reg events:"
    ls "$T/events/amdgpu" 2>/dev/null | grep -iE 'reg' | sed 's/^/  amdgpu\//'
    exit 1
}

cleanup() { echo 0 > "$T/tracing_on" 2>/dev/null || true; echo 0 > "$T/events/$EV/enable" 2>/dev/null || true; }
trap cleanup EXIT INT TERM

echo 0 > "$T/tracing_on"
echo 1 > "$T/events/$EV/enable"
: > "$T/trace"

cat <<'EOF'

=== amdgpu HDMI modeset clock/PHY capture ===
This traces every amdgpu host MMIO write. To capture the COLD HDMI bring-up:
  1. Press Enter to START tracing.
  2. UNPLUG the HDMI cable from the XB323U.
  3. Wait ~3 seconds.
  4. PLUG it back in and wait for the picture to return.
  5. Press Enter to STOP.
(Physical unplug/replug forces amdgpu's full link re-bring-up — the real modeset.)

EOF
printf 'Press Enter to START tracing... '; read _ || true
echo 1 > "$T/tracing_on"
echo '>>> TRACING. Unplug HDMI, wait ~3s, replug, wait for picture, then press Enter.'
read _ || true
echo 0 > "$T/tracing_on"

cp "$T/trace" "$OUT"
[ -n "${SUDO_USER:-}" ] && { chown "$SUDO_USER" "$OUT" 2>/dev/null || true; chmod 644 "$OUT" 2>/dev/null || true; }

TOT=$(grep -c 'amdgpu_device_wreg' "$OUT" 2>/dev/null || echo 0)
echo ""
echo "captured $TOT total register writes -> $OUT"
echo ""

# --- Post-analysis: strip compositor page-flip/cursor noise, show the modeset ---
# Noise registers (identified in P5a as an idle compositor flipping every vblank):
#   HUBPREQ page-flip/cursor 0x3d5d-0x3d70, OTG-double-buffer 0x504a/0x504b/0x5023,
#   0x3ae0, 0x4ffa. These fire hundreds of times; the modeset writes fire 1-3 times.
echo "=== MODESET writes (compositor noise stripped), in order, adjacent-deduped ==="
grep 'amdgpu_device_wreg' "$OUT" | awk '
  { # format: ... amdgpu_device_wreg: 0xDID, 0xREG, 0xVALUE
    if (match($0, /amdgpu_device_wreg: 0x[0-9a-fA-F]+, 0x[0-9a-fA-F]+, 0x[0-9a-fA-F]+/)) {
      n=split(substr($0,RSTART),a,", ");
      reg=a[2]; val=a[3]; sub(/[^0-9a-fA-Fx].*$/,"",val);
      d=strtonum(reg);
      # skip the known high-frequency compositor registers
      if (d>=0x3d5d && d<=0x3d70) next;
      if (d==0x504a || d==0x504b || d==0x5023 || d==0x3ae0 || d==0x4ffa) next;
      # classify by region (absolute dword index)
      region="other";
      if (d<0x400)                      region="DCCG/CLOCK(seg1)";
      else if (d>=0x1b00 && d<=0x1c00)  region="OTG/OPTC";
      else if (d>=0x5400 && d<=0x5900)  region="DIG/AFMT/AZ(audio)";
      else if (d>=0x5500 && d<=0x56ff)  region="UNIPHY";
      else if (d>=0x5c00 && d<=0x5fff)  region="RDPCS/DPCS-PHY";
      else if (d>=0x6700 && d<=0x6800)  region="DMCUB-INBOX";
      line=sprintf("reg=0x%06x val=%-10s [%s]", d, val, region);
      if (line!=prev) printf "  %s\n", line;
      prev=line;
    }
  }'

echo ""
echo "=== KEY QUESTION 1 — is the pixel clock host-programmed? (DCCG/CLOCK writes, seg1 <0x400) ==="
grep 'amdgpu_device_wreg' "$OUT" | awk '{if(match($0,/wreg: 0x[0-9a-fA-F]+, 0x([0-9a-fA-F]+), 0x[0-9a-fA-F]+/,m)){d=strtonum("0x"m[1]); if(d<0x400) print}}' | sort -u | head -40
echo "  (nonzero ⇒ agnos can replicate the clock directly; empty ⇒ clock is DMCUB-opaque)"

echo ""
echo "=== KEY QUESTION 2 — does the transmitter/clock go through the DMCUB inbox? (0x67xx) ==="
grep 'amdgpu_device_wreg' "$OUT" | awk '{if(match($0,/wreg: 0x[0-9a-fA-F]+, 0x([0-9a-fA-F]+), 0x[0-9a-fA-F]+/,m)){d=strtonum("0x"m[1]); if(d>=0x6700&&d<=0x6800) print}}' | wc -l | sed 's/^/  DMCUB-inbox writes: /'
echo "  (many ⇒ firmware path, PHY/clock opaque; few/none ⇒ host-visible, we can replicate)"

echo ""
echo "=== KEY QUESTION 3 — PHY writes present? (RDPCS/DPCS 0x5c00-0x5fff, UNIPHY 0x55xx) ==="
grep 'amdgpu_device_wreg' "$OUT" | awk '{if(match($0,/wreg: 0x[0-9a-fA-F]+, 0x([0-9a-fA-F]+), 0x[0-9a-fA-F]+/,m)){d=strtonum("0x"m[1]); if((d>=0x5c00&&d<=0x5fff)||(d>=0x5500&&d<=0x56ff)) print}}' | sort -u | head -30
echo "  (present ⇒ host-ATOM-style, replayable; absent ⇒ confirms DMCUB firmware transmitter)"

echo ""
echo "done. Send me $OUT"
