#!/bin/bash
# capture-amdgpu-audio-writes.sh — record EVERY display register amdgpu writes (offset + value,
# in order) during an HDMI audio start and/or a modeset, plus the HDA codec verbs ALSA sends.
# This is the one thing the steady-state known-good dump cannot show: the SEQUENCE of writes and
# any register OUTSIDE the ~40 our dump covers.
#
# WHY: AGNOS matches amdgpu's steady-state DCN-audio registers byte-for-byte and is still silent.
# AGNOS brings HDMI up with a LIVE DIG_MODE 2->3 flip; amdgpu does a COLD bring-up at modeset.
# Whatever amdgpu writes that AGNOS never does is in that sequence. This captures it.
#
# HOW: uses amdgpu's OWN built-in ftrace tracepoints — no kprobe, no guessing:
#   amdgpu:amdgpu_dc_wreg     — every DISPLAY (DC/DCN) register write: "reg=0x.., value=0x.."
#   amdgpu:amdgpu_device_wreg — every core amdgpu MMIO write (complement)
#   hda:hda_send_cmd          — every codec verb snd_hda_intel sends the HDMI codec (04:00.1)
# It only OBSERVES what amdgpu/ALSA write; it writes no GPU register itself. Safe on the live desktop.
# Requires root (tracefs). Output: ./amdgpu-writes-<mode>.txt
#
# Usage:
#   sudo ./capture-amdgpu-audio-writes.sh modeset   # HIGHEST value: unplug/replug HDMI while tracing
#   sudo ./capture-amdgpu-audio-writes.sh audio     # play a tone to the HDMI sink while tracing
set -u

MODE="${1:-audio}"
OUT="amdgpu-writes-${MODE}.txt"
[ "$(id -u)" = "0" ] || { echo "run with sudo"; exit 1; }

# --- locate tracefs -------------------------------------------------------
T=/sys/kernel/tracing
[ -d "$T/events" ] || T=/sys/kernel/debug/tracing
[ -d "$T/events" ] || mount -t tracefs nodev /sys/kernel/tracing 2>/dev/null && T=/sys/kernel/tracing
[ -w "$T/tracing_on" ] || { echo "tracefs not writable at $T"; exit 1; }

# --- pick the tracepoints that exist on this kernel -----------------------
EVENTS=""
add_ev() { [ -e "$T/events/$1/enable" ] && EVENTS="$EVENTS $1"; }
add_ev amdgpu/amdgpu_dc_wreg          # DISPLAY register writes (the ones we care about)
add_ev amdgpu/amdgpu_device_wreg      # core MMIO writes (complement)
add_ev hda/hda_send_cmd               # codec verbs to 04:00.1
add_ev hda/hda_get_response
if [ -z "$EVENTS" ]; then
    echo "None of the expected tracepoints exist here. Available amdgpu/hda events:"
    ls "$T/events/amdgpu" 2>/dev/null | grep -iE 'wreg|reg' | sed 's/^/   amdgpu\//'
    ls "$T/events/hda"    2>/dev/null | sed 's/^/   hda\//'
    echo "Tell me which are listed and I'll adjust."
    exit 1
fi
echo "tracing:$EVENTS"

cleanup() {
    echo 0 > "$T/tracing_on" 2>/dev/null
    for e in $EVENTS; do echo 0 > "$T/events/$e/enable" 2>/dev/null; done
}
trap cleanup EXIT

echo 0 > "$T/tracing_on"
for e in $EVENTS; do echo 1 > "$T/events/$e/enable"; done
echo > "$T/trace"

echo ""
if [ "$MODE" = "modeset" ]; then
    cat <<'EOF'
=== MODESET capture (highest value — captures the cold HDMI bring-up) ===
On START, force amdgpu to re-bring-up the HDMI link:
    UNPLUG the HDMI cable, wait ~2s, PLUG it back in       (best — full ATOM bring-up)
  or:  xrandr --output <HDMI-out> --off ; sleep 2 ; xrandr --output <HDMI-out> --auto
EOF
else
    cat <<'EOF'
=== AUDIO capture ===
On START, play a tone to the HDMI sink and CONFIRM you hear it:
    ffmpeg -f lavfi -i "sine=frequency=440:duration=3" -ac 2 -ar 48000 -f alsa hw:0,7
  or:  speaker-test -D hw:0,7 -c 2 -t sine -f 440 -l 1
EOF
fi
read -r -p "Press Enter to START tracing... " _
echo 1 > "$T/tracing_on"
echo ">>> tracing... do the action now, then press Enter to STOP."
read -r _
echo 0 > "$T/tracing_on"

cp "$T/trace" "$OUT"
# the script runs as root, so hand the output back to the invoking user (readable without sudo)
[ -n "${SUDO_USER:-}" ] && chown "$SUDO_USER" "$OUT" 2>/dev/null && chmod 644 "$OUT" 2>/dev/null
NW=$(grep -cE 'amdgpu_dc_wreg|amdgpu_device_wreg' "$OUT" 2>/dev/null || echo 0)
NV=$(grep -cE 'hda_send_cmd' "$OUT" 2>/dev/null || echo 0)
echo ""
echo "captured $NW register writes + $NV codec verbs -> $OUT"
echo ""
if [ "$MODE" = "modeset" ]; then
    # === P5a: the TRANSMITTER bring-up. The audio region is already byte-matched; what we need is the
    # encoder + PHY writes agnos never does. Show EVERY dc_wreg + device_wreg write, grouped by region,
    # so the DIG back-end / UNIPHY / DCIO / DCCG writes (the transmitter enable) are not filtered out.
    # SEG[2]=0x34C0: audio/DIG dword region is ~0x5400-0x5900. Anything OUTSIDE that (esp. the PHY) is the
    # P5a prize. amdgpu's ATOM TransmitterControl writes may land in device_wreg, not dc_wreg — capture both.
    echo "=== ALL register writes during the modeset, in ORDER (dc_wreg + device_wreg), deduped-adjacent ==="
    grep -E 'amdgpu_dc_wreg|amdgpu_device_wreg' "$OUT" | awk '
      { r=""; v=""; src="dc"; if ($0 ~ /device_wreg/) src="dev";
        if (match($0,/reg=0x[0-9a-fA-F]+/))   r=substr($0,RSTART+4,RLENGTH-4);
        if (match($0,/value=0x[0-9a-fA-F]+/)) v=substr($0,RSTART+6,RLENGTH-6);
        if (r=="") next; d=strtonum("0x" r);
        region="OTHER";
        if (d>=0x5400 && d<=0x5900) region="DIG/AFMT/AZ(audio-matched)";
        else if (d>=0xC0 && d<=0x200) region="DCCG/OTG-rate(seg1)";
        else if (d>=0x6C00 && d<=0x6F00) region="OTG/OPTC(seg2)";
        line=sprintf("%-4s reg=0x%06x value=0x%-8s [%s]", src, d, v, region);
        if (line!=prev) printf "  %s\n", line; prev=line; }'
    echo ""
    echo "=== write COUNT by region (where is the transmitter bring-up?) ==="
    grep -E 'amdgpu_dc_wreg|amdgpu_device_wreg' "$OUT" | awk '
      { if (match($0,/reg=0x[0-9a-fA-F]+/)) { d=strtonum("0x" substr($0,RSTART+4,RLENGTH-4));
          if (d>=0x5400 && d<=0x5900) a++; else if (d>=0xC0 && d<=0x200) c++;
          else if (d>=0x6C00 && d<=0x6F00) o++; else other++; } }
      END { printf "  audio(0x5400-5900)=%d  dccg(0xC0-200)=%d  otg(0x6C00-6F00)=%d  OTHER(PHY/misc)=%d\n",
                   a,c,o,other }'
else
    echo "=== display register writes in the DCN encoder/audio region (dword 0x5400-0x5900 = DIG/AFMT/AZ) ==="
    grep -E 'amdgpu_dc_wreg' "$OUT" | awk '
      { r=""; v="";
        if (match($0,/reg=0x[0-9a-fA-F]+/))   r=substr($0,RSTART+4,RLENGTH-4);
        if (match($0,/value=0x[0-9a-fA-F]+/)) v=substr($0,RSTART+6,RLENGTH-6);
        if (r!="") { d=strtonum("0x" r);
          if ((d>=0x5400 && d<=0x5900) || (d>=0xC0 && d<=0x200))
            printf "  reg=0x%05x  value=0x%s\n", d, v; } }' | head -80
fi
echo ""
echo "=== HDA codec verbs during the capture ==="
grep -E 'hda_send_cmd' "$OUT" | head -60
echo ""
echo "Full trace in $OUT — send it and I'll map every offset to its DCN register name, isolate the"
echo "transmitter/encoder/PHY writes (the P5a prize), and turn them into agnos's sovereign bring-up."
