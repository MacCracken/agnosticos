#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# ramp-probe.sh — read, and then PERTURB, the AFMT RAMP block: the last register set never read on EITHER
#                 path, and the only one that is literally an attenuator.
#
# WHY
#   The perturbation test just closed "agnos has a wrong register VALUE" by demonstration: all six measured
#   differences were forced onto the working amdgpu path and NONE killed the tone, and none drove tap1 to
#   zero. So agnos's silence is not caused by any difference we have MEASURED.
#
#   But five DIG registers have never been read on EITHER path, so they are not in the diff at all:
#       AFMT_RAMP_CONTROL0  0x20A3   RAMP_MAX_COUNT[23:0], RAMP_DATA_SIGN[31]
#       AFMT_RAMP_CONTROL1  0x20A4   RAMP_MIN_COUNT[23:0], AFMT_AUDIO_TEST_CH_DISABLE[31:24]
#       AFMT_RAMP_CONTROL2  0x20A5   RAMP_INC_COUNT[23:0]
#       AFMT_RAMP_CONTROL3  0x20A6   RAMP_DEC_COUNT[23:0]
#       AFMT_INTERRUPT_STATUS 0x2079
#
#   A RAMP IS AN ATTENUATOR, and RAMP_CONTROL1 carries a per-channel DISABLE. A ramp parked at zero would
#   attenuate real content to EXACTLY zero — agnos's signature — and would appear in NO diff, because agnos
#   never reads it and neither did the known-good capture.
#
#   An audit refuted this block BY ARGUMENT ("gated by AFMT_AUDIO_TEST_EN, which reads 0 on both paths;
#   amdgpu never writes them"). That is an inference from a field name. Six such inferences were just tested
#   against hardware and survived only because they happened to be right. This block has never been LOOKED
#   AT. Look at it.
#
# WHAT THIS DOES
#   1. Reads the RAMP block on the AUDIBLY WORKING amdgpu path (with a tone playing). If agnos's values
#      differ, that is a NEW DIFF in the last unread block.
#   2. PERTURBS it — zeroes the ramp — and asks whether the tone dies. If it does, the block is LIVE and
#      "gated by TEST_EN" is refuted by demonstration, whatever the field name says.
#
# SAFETY: audio registers only. Restored unconditionally on exit including Ctrl-C. No modeset, no PHY,
# nothing that can black-screen the console.
#
# USAGE: wake the display, volume up.  sudo ./scripts/ramp-probe.sh
#
set -eu
REPO="$(cd "$(dirname "$0")/.." && pwd)"
DUMP="$REPO/scripts/dump-dcn-audio.py"
CARD=0; DIG=1
OUT="$REPO/ramp-probe-$(date +%Y-%m-%d-%H%M).txt"
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

# RAMP dwords for DIG1 = base + 0x100
R0=$((0x20A3 + 0x100)); R1=$((0x20A4 + 0x100)); R2=$((0x20A5 + 0x100)); R3=$((0x20A6 + 0x100))
SAVED=""
TONE_PID=""
restore() {
    [ -n "$SAVED" ] && eval "$SAVED" >/dev/null 2>&1 || true
    [ -n "$TONE_PID" ] && kill "$TONE_PID" 2>/dev/null
    [ -n "${SUDO_UID:-}" ] && chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$OUT" 2>/dev/null || true
}
trap restore EXIT INT TERM

echo "panel on hw:$CARD,$PIN_DEV — DIG$DIG"
echo "Starting a 440 Hz tone. IT SHOULD BE AUDIBLE."
ffmpeg -hide_banner -loglevel error -f lavfi -i "sine=frequency=440:duration=600:sample_rate=48000" \
    -af "volume=0.30" -ac 2 -ar 48000 -f alsa "hw:$CARD,$PIN_DEV" >/dev/null 2>&1 &
TONE_PID=$!
sleep 3
kill -0 $TONE_PID 2>/dev/null || { echo "error: ALSA refused the stream" >&2; exit 1; }

echo
echo "=== THE RAMP BLOCK ON THE WORKING PATH (never read before, on either path) ==="
"$DUMP" 2>/dev/null | grep -E "DIG1_(AFMT_RAMP_CONTROL[0-3]|AFMT_INTERRUPT_STATUS)" | sed 's/^/  /'
echo
echo "  (agnos has NEVER read these either — if they differ, this is the last unread block in the diff)"

# capture originals for restore
V0=$("$DUMP" 2>/dev/null | awk '/DIG1_AFMT_RAMP_CONTROL0/{print $3}')
V1=$("$DUMP" 2>/dev/null | awk '/DIG1_AFMT_RAMP_CONTROL1/{print $3}')
V2=$("$DUMP" 2>/dev/null | awk '/DIG1_AFMT_RAMP_CONTROL2/{print $3}')
V3=$("$DUMP" 2>/dev/null | awk '/DIG1_AFMT_RAMP_CONTROL3/{print $3}')
SAVED="\"$DUMP\" --reg-write 2:$R0:$V0 --reg-write 2:$R1:$V1 --reg-write 2:$R2:$V2 --reg-write 2:$R3:$V3"
echo "  saved originals: $V0 $V1 $V2 $V3"

taps() {
    for s in 0 1; do
        v=$("$DUMP" --crc "$DIG" --crc-source "$s" --crc-count 2048 2>/dev/null | grep -E "CRC_DONE=" | head -1)
        echo "    tap$s: ${v:-<none>}"
    done
}
ask() {
    printf "\n  >>> Tone still audible? [y / n = IT DIED] > " >/dev/tty
    read -r a </dev/tty || a="?"
    case "$a" in
        n|N) echo "  *** THE TONE DIED — THE RAMP BLOCK IS LIVE. This is the bug. ***" ;;
        y|Y) echo "  (still audible)" ;;
        *)   echo "  (unanswered)" ;;
    esac
}

echo
echo "=== BASELINE ==="; taps

echo
echo "================================================================"
echo " PERTURB 1: zero the whole RAMP block (MAX/MIN/INC/DEC = 0)"
echo "================================================================"
"$DUMP" --reg-write "2:$R0:0" --reg-write "2:$R1:0" --reg-write "2:$R2:0" --reg-write "2:$R3:0" 2>&1 \
    | grep -E "wrote" | sed 's/^/  /'
sleep 2; taps; ask
eval "$SAVED" >/dev/null 2>&1; sleep 1

echo
echo "================================================================"
echo " PERTURB 2: AFMT_AUDIO_TEST_CH_DISABLE = 0xFF (RAMP_CONTROL1 [31:24])"
echo "================================================================"
NEW1=$(printf "0x%08x" $(( (0xFF << 24) | ($(printf "%d" "$V1") & 0x00FFFFFF) )))
"$DUMP" --reg-write "2:$R1:$NEW1" 2>&1 | grep -E "wrote" | sed 's/^/  /'
sleep 2; taps; ask
eval "$SAVED" >/dev/null 2>&1; sleep 1

echo
echo "=== RESTORED — tone should be back ==="; taps
echo
cat <<'EOF'
=== HOW TO READ THIS ===
  EITHER perturbation kills the tone -> the RAMP block is LIVE on this hardware, "gated by
  AFMT_AUDIO_TEST_EN" is refuted by demonstration, and agnos's unread RAMP values are the prime suspect.
  Next: add the RAMP block to agnos's gpu_audio_dump and compare.

  Neither kills it, and the values above match agnos's -> the block is genuinely inert, the register file
  is exhausted for real, and the cause is SEQUENCE or STATE, not any value.

  Either way this is the last unread register block. After it, "wrong value somewhere" is finished.
EOF