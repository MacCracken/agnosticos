#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# replay-agnos-sequence.sh — replay agnos's WRITE SEQUENCE onto the audibly-working amdgpu path, in stages,
#                            and find which stage kills the sound. YOUR EARS ARE THE ORACLE.
#
# WHY THIS IS THE LAST IDEA THAT MAKES SENSE
#   The register-VALUE space is now exhausted BY DEMONSTRATION, not argument:
#     - every AFMT/DIG/DCCG/AZ control register is byte-identical between agnos and an audibly-working amdgpu
#     - all six measured differences were FORCED onto the working path: none killed the tone, none drove
#       tap1 to zero
#     - the last unread block (AFMT_RAMP_CONTROL0..3 + AFMT_INTERRUPT_STATUS) reads ALL ZERO on the working
#       path and zeroing/perturbing it changes nothing — it is inert
#   There is no wrong value. That theory is dead.
#
#   What is left is ORDER. agnos and amdgpu reach the SAME final register state by DIFFERENT sequences —
#   which is exactly why every diff came back clean. Known ordering deltas (from agnos gpu.cyr
#   gpu_hdmi_audio_enable vs amdgpu core_link_enable_stream -> dcn10_link_encoder_setup ->
#   enc1_stream_encoder_hdmi_set_stream_attribute -> audio setup):
#     agnos:  HDMI_CONTROL, HDMI_GC, HDMI_VBI  ->  DIG_MODE=3  ->  DTO -> AZ -> AFMT
#     amdgpu: DIG_MODE=3  ->  HDMI_CONTROL ... HDMI_GC  ->  DTO -> AZ -> AFMT
#   agnos programs the HDMI packet block BEFORE the mode flip; amdgpu programs it AFTER.
#
#   If any write in agnos's sequence latches state that a LATER write cannot undo, the final registers read
#   identical and the hardware behaves differently. That is precisely agnos's signature: real samples in,
#   digital silence out, every control register matching.
#
# THE TEST
#   Replay agnos's sequence, in stages, onto a path that is CURRENTLY MAKING SOUND. Ask after each stage.
#   The stage that kills the tone contains the bug — no inference, no exoneration chain, no CRC-tap
#   interpretation. Then bisect inside that stage.
#
#   If NOTHING kills it: agnos's sequence is harmless too, and the cause is not in the DIG/AFMT/AZ/DCCG
#   surface at all. That is a real result and it redirects the whole arc.
#
# SAFETY
#   Audio-path registers plus a DIG_MODE 3->3 rewrite (same value amdgpu already has — no mode change).
#   No PHY, no link training, no OTG, no modeset. Worst case is silence until restore. Everything is
#   captured up front and restored unconditionally on exit including Ctrl-C.
#
# USAGE: wake the display, volume up.  sudo ./scripts/replay-agnos-sequence.sh
#
set -eu
REPO="$(cd "$(dirname "$0")/.." && pwd)"
D="$REPO/scripts/dump-dcn-audio.py"
CARD=0; DIG=1; EP=1
OUT="$REPO/replay-$(date +%Y-%m-%d-%H%M).txt"
[ "$(id -u)" -eq 0 ] || { echo "error: needs root — sudo" >&2; exit 1; }
exec > >(tee "$OUT") 2>&1

PIN_DEV=""
for p in /proc/asound/card$CARD/pcm*p; do
    [ -r "$p/info" ] || continue
    k=$(awk '/^id: *HDMI /{print $3; exit}' "$p/info" 2>/dev/null); [ -n "$k" ] || continue
    if [ "$(awk '/^monitor_present/{print $2}' "/proc/asound/card$CARD/eld#0.$k" 2>/dev/null)" = "1" ]; then
        PIN_DEV=$(basename "$p" | sed 's/^pcm//; s/p$//'); break
    fi
done
[ -n "$PIN_DEV" ] || { echo "error: no HDMI pin present — panel asleep?" >&2; exit 1; }

# DIG1 dword offsets (DIG0 base + 0x100)
HDMI_CONTROL=$((0x2071+0x100)); HDMI_GC=$((0x207B+0x100)); HDMI_VBI=$((0x2075+0x100))
DIG_BE=$((0x20AF+0x100));       AFMT_CNTL=$((0x20E6+0x100))
AFMT_APC=$((0x20AA+0x100));     AFMT_APC2=$((0x207C+0x100)); AFMT_SRC=$((0x20AD+0x100))
HDMI_APC=$((0x2073+0x100));     HDMI_ACR_PC=$((0x2074+0x100))

O_HC=$("$D" 2>/dev/null | awk '/DIG1_HDMI_CONTROL/{print $3}')
O_GC=$("$D" 2>/dev/null | awk '/DIG1_HDMI_GC/{print $3}')
O_VBI=$("$D" 2>/dev/null | awk '/DIG1_HDMI_VBI_PACKET_CONTROL/{print $3}')
O_BE=$("$D" 2>/dev/null | awk '/DIG1_DIG_BE_CNTL/{print $3}')
O_ACNTL=$("$D" 2>/dev/null | awk '/DIG1_AFMT_CNTL/{print $3}')
O_APC=$("$D" 2>/dev/null | awk '/DIG1_AFMT_AUDIO_PACKET_CONTROL /{print $3}')
O_APC2=$("$D" 2>/dev/null | awk '/DIG1_AFMT_AUDIO_PACKET_CONTROL2/{print $3}')
O_SRC=$("$D" 2>/dev/null | awk '/DIG1_AFMT_AUDIO_SRC_CONTROL/{print $3}')
O_HAPC=$("$D" 2>/dev/null | awk '/DIG1_HDMI_AUDIO_PACKET_CONTROL/{print $3}')
O_ACR=$("$D" 2>/dev/null | awk '/DIG1_HDMI_ACR_PACKET_CONTROL/{print $3}')
O_DTOS=$("$D" 2>/dev/null | awk '/DCCG_AUDIO_DTO_SOURCE/{print $3}')
O_DTOM=$("$D" 2>/dev/null | awk '/DCCG_AUDIO_DTO0_MODULE/{print $3}')
O_DTOP=$("$D" 2>/dev/null | awk '/DCCG_AUDIO_DTO0_PHASE/{print $3}')

TONE_PID=""
restore() {
    "$D" --reg-write "2:$HDMI_CONTROL:$O_HC" --reg-write "2:$HDMI_GC:$O_GC" \
         --reg-write "2:$HDMI_VBI:$O_VBI"    --reg-write "2:$DIG_BE:$O_BE" \
         --reg-write "2:$AFMT_CNTL:$O_ACNTL" --reg-write "2:$AFMT_APC:$O_APC" \
         --reg-write "2:$AFMT_APC2:$O_APC2"  --reg-write "2:$AFMT_SRC:$O_SRC" \
         --reg-write "2:$HDMI_APC:$O_HAPC"   --reg-write "2:$HDMI_ACR_PC:$O_ACR" \
         --reg-write "1:0x00AB:$O_DTOS" --reg-write "1:0x00AD:$O_DTOM" --reg-write "1:0x00AC:$O_DTOP" \
         >/dev/null 2>&1 || true
    [ -n "$TONE_PID" ] && kill "$TONE_PID" 2>/dev/null
    [ -n "${SUDO_UID:-}" ] && chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$OUT" 2>/dev/null || true
}
trap restore EXIT INT TERM

echo "panel hw:$CARD,$PIN_DEV — DIG$DIG, endpoint $EP"
echo "originals: HC=$O_HC GC=$O_GC VBI=$O_VBI BE=$O_BE AFMT_CNTL=$O_ACNTL APC=$O_APC APC2=$O_APC2 SRC=$O_SRC"
echo "           HAPC=$O_HAPC ACR=$O_ACR DTO=$O_DTOS/$O_DTOM/$O_DTOP"
echo
echo "Starting 440 Hz tone — SHOULD BE AUDIBLE."
ffmpeg -hide_banner -loglevel error -f lavfi -i "sine=frequency=440:duration=900:sample_rate=48000" \
    -af "volume=0.30" -ac 2 -ar 48000 -f alsa "hw:$CARD,$PIN_DEV" >/dev/null 2>&1 &
TONE_PID=$!
sleep 3
kill -0 $TONE_PID 2>/dev/null || { echo "error: ALSA refused the stream" >&2; exit 1; }

taps() {
    for s in 0 1; do
        v=$("$D" --crc "$DIG" --crc-source "$s" --crc-count 2048 2>/dev/null | grep -E "CRC_DONE=" | head -1)
        echo "    tap$s: ${v:-<none>}"
    done
}
ask() {
    printf "\n  >>> Tone still audible? [y / n = DIED] > " >/dev/tty
    read -r a </dev/tty || a="?"
    case "$a" in
        n|N) echo "  *** DIED HERE — THIS STAGE CONTAINS THE BUG ***" ;;
        y|Y) echo "  (still audible)" ;;
        *)   echo "  (unanswered)" ;;
    esac
}

echo; echo "=== BASELINE ==="; taps

echo; echo "================================================================"
echo " STAGE 1 — agnos's ORDER: HDMI_CONTROL, HDMI_GC, HDMI_VBI written BEFORE the DIG_MODE flip"
echo "           (amdgpu writes DIG_MODE FIRST, then these)"
echo "================================================================"
"$D" --reg-write "2:$HDMI_CONTROL:0x10019" --reg-write "2:$HDMI_GC:0x4" \
     --reg-write "2:$HDMI_VBI:0x31" 2>&1 | grep wrote | sed 's/^/  /'
sleep 1
echo "  ...now the DIG_MODE=3 rewrite (same value amdgpu already has — tests whether the WRITE re-latches)"
"$D" --reg-write "2:$DIG_BE:0x10030200" 2>&1 | grep wrote | sed 's/^/  /'
sleep 2; taps; ask

echo; echo "================================================================"
echo " STAGE 2 — agnos's DCCG audio DTO sequence (SOURCE, then MODULE, then PHASE)"
echo "================================================================"
"$D" --reg-write "1:0x00AB:0x0" --reg-write "1:0x00AD:0x24D9B8" --reg-write "1:0x00AC:0x3a980" 2>&1 \
    | grep wrote | sed 's/^/  /'
sleep 2; taps; ask

echo; echo "================================================================"
echo " STAGE 3 — agnos's AZ endpoint sequence (clock-gate bracket, speaker, enable, slot map)"
echo "================================================================"
"$D" --az-write "$EP:0x54:0x80000001" --az-write "$EP:0x25:0x10001" \
     --az-write "$EP:0x54:0x80000001" --az-write "$EP:0x36:0x1" \
     --az-write "$EP:0x54:0x80000000" 2>&1 | grep -E "wrote" | sed 's/^/  /'
sleep 2; taps; ask

echo; echo "================================================================"
echo " STAGE 4 — agnos's AFMT sequence (clock, src select, channel enable, ACR, sample send LAST)"
echo "================================================================"
"$D" --reg-write "2:$AFMT_CNTL:0x101" --reg-write "2:$AFMT_SRC:0x1" \
     --reg-write "2:$AFMT_APC2:0x300" --reg-write "2:$HDMI_ACR_PC:0x11000" \
     --reg-write "2:$HDMI_APC:0x10" --reg-write "2:$AFMT_APC:0x801" 2>&1 | grep wrote | sed 's/^/  /'
sleep 2; taps; ask

echo; echo "=== RESTORING ==="
"$D" --reg-write "2:$HDMI_CONTROL:$O_HC" --reg-write "2:$HDMI_GC:$O_GC" --reg-write "2:$HDMI_VBI:$O_VBI" \
     --reg-write "2:$DIG_BE:$O_BE" --reg-write "2:$AFMT_CNTL:$O_ACNTL" --reg-write "2:$AFMT_APC:$O_APC" \
     --reg-write "2:$AFMT_APC2:$O_APC2" --reg-write "2:$AFMT_SRC:$O_SRC" \
     --reg-write "2:$HDMI_APC:$O_HAPC" --reg-write "2:$HDMI_ACR_PC:$O_ACR" \
     --reg-write "1:0x00AB:$O_DTOS" --reg-write "1:0x00AD:$O_DTOM" --reg-write "1:0x00AC:$O_DTOP" \
     >/dev/null 2>&1
sleep 1; taps
echo
cat <<'EOF'
=== HOW TO READ THIS ===
  A stage that KILLS the tone contains the bug, by demonstration. Then we bisect inside it — each stage is
  only 3-6 writes.

  Nothing kills it -> agnos's sequence is ALSO harmless on working hardware. Combined with the value tests,
  that means NOTHING agnos does to the DIG/AFMT/AZ/DCCG surface can explain the silence, and the difference
  must be something agnos does NOT do (a write it never makes) or something outside this surface entirely.
  That closes the whole "agnos programs it wrong" theory and redirects the arc.

  Watch tap1: anything that drives it to CRC=0x000000 while the counters still complete reproduces agnos's
  exact signature — that is the jackpot even if the tone somehow survives.
EOF