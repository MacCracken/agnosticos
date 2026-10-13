#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# crc-model-discriminator.sh — falsify the instrument the whole display-audio arc rests on.
#
# THE PROBLEM
#   agnos reads, on its silent path:   tap source=0 -> DONE=1 CRC=0x25EB75 (content)
#                                      tap source=1 -> DONE=1 CRC=0x000000 (zero)
#   and the arc concluded "real samples enter the AFMT and are ZEROED inside it".
#
#   AFMT_AUDIO_CRC_CONTROL is ONE engine: one enable, one counter, one CRC shift register, and CRC_SOURCE is
#   a SINGLE BIT (mask 0x100) selecting between two inputs. amdgpu's DC never reads, writes, or references
#   this block anywhere — there is ZERO source ground truth. Two models fit:
#
#     MODEL A  the counter is gated by sample-valid AT THE SELECTED TAP.
#              => agnos's tap-1 zeros mean a RUNNING output stage is emitting silence.
#              => something zeroes the data. (What the arc assumed.)
#     MODEL B  ONE counter gated by the SHARED INPUT strobe; SOURCE only muxes which data bus feeds the CRC.
#              => agnos's tap-1 zeros mean "2048 samples ENTERED, and the tap-1 bus read zero throughout".
#              => the output stage NEVER FIRES. Samples never leave. Nothing zeroes anything.
#
#   THE THREE STIMULI ALREADY RUN DO NOT DISCRIMINATE:
#       nothing playing  -> DONE=0 / DONE=0        under BOTH models
#       tone             -> two non-zero CRCs      under BOTH models
#       digital silence  -> 0x000000 / 0x000000    under BOTH models
#   Three points, zero discrimination. The state that separates them — INPUT FLOWING, OUTPUT STOPPED — does
#   not occur naturally on a working path and has never been measured.
#
#   And the corpus already leans MODEL B, via a datum the arc dismissed CIRCULARLY: AFMT_STATUS bit24
#   (FIFO_OVERFLOW) is agnos=1 / audibly-working-amdgpu=0 — the ONLY audio-side difference in the whole
#   capture. A FIFO overflows when fill outpaces drain. Under Model A the drain runs at 48 kHz and has NO
#   REASON to overflow. The arc reclassified it as "symptom — the FIFO is full of real zero-samples", using
#   the contested Model A reading of the CRC to discard the one measurement that discriminates against
#   Model A.
#
# THE EXPERIMENT
#   Manufacture "input flowing, output stopped" on the AUDIBLY WORKING amdgpu path, using the encoder's own
#   mute, and read both taps.
#
#   The lever is amdgpu's: link_hwss_dio.c enable_dio_audio_packet() -> for HDMI the entire drain-side
#   enable is one call, audio_mute_control(enc, false). dcn10_stream_encoder.c enc1_se_audio_mute_control()
#   is literally REG_UPDATE(AFMT_AUDIO_PACKET_CONTROL, AFMT_AUDIO_SAMPLE_SEND, !mute), and
#   dcn20_stream_encoder.c binds that same function — Cezanne's live path. Bit 0 of DIG1's 0x20AA is the
#   hardware's own output-stage switch, toggled on every mute, fully reversible.
#
#   PREDICTIONS ARE OPPOSITE:
#     source=1 -> DONE=0 (never completes)   => MODEL A. The counter is per-tap. agnos's premise SURVIVES:
#                                               real samples are being zeroed, and the hunt continues.
#     source=1 -> DONE=1, CRC=0x000000       => MODEL B. agnos's EXACT SIGNATURE, reproduced on hardware
#                                               that is merely MUTED and is definitely NOT passing 2048 zero
#                                               samples. "CRC=0 means zeros flowed" is FALSIFIED BY
#                                               DEMONSTRATION, and the target moves to "why does the
#                                               audio-sample-packet inserter never fire?"
#     BONUS: if AFMT_STATUS bit24 also SETS under mute, the good-path mute reproduces agnos's ENTIRE
#            signature (content at tap0 + DONE=1/CRC=0 at tap1 + FIFO_OVERFLOW=1) and the overflow stops
#            being a symptom and becomes the DIAGNOSIS.
#
#   THE CONFOUND (controlled below): 0x801 is bit0 (SAMPLE_SEND) PLUS bit11
#   (AFMT_RESET_FIFO_WHEN_AUDIO_DIS). Clearing bit0 while bit11 is set may hold the AFMT FIFO in reset —
#   which is NOT agnos's state (agnos's FIFO is OVERFLOWING, i.e. NOT reset). So run the 2x2: mute with
#   bit11 set, and mute with bit11 also cleared.
#
# RISK: near-zero. Writes NOTHING to agnos. On the host it is a momentary mute of a test tone via the exact
# register amdgpu itself toggles on every mute — no modeset, no link change, no persistent state. The
# original value is restored unconditionally on exit, including on Ctrl-C.
#
# USAGE
#   Wake the display first. sudo ./scripts/crc-model-discriminator.sh
#   Writes crc-model-<date>.txt AND prints to the terminal — no piping needed.
#
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
DUMP="$REPO/scripts/dump-dcn-audio.py"
CARD=0
DIG=1
OUT="$REPO/crc-model-$(date +%Y-%m-%d-%H%M).txt"

for a in "$@"; do
    case "$a" in
        --out=*) OUT="${a#--out=}" ;;
        *) echo "unknown option: $a" >&2; exit 2 ;;
    esac
done

[ "$(id -u)" -eq 0 ] || { echo "error: needs root — re-run with sudo" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "error: ffmpeg not found" >&2; exit 1; }
[ -x "$DUMP" ] || { echo "error: $DUMP missing" >&2; exit 1; }

exec > >(tee "$OUT") 2>&1

PIN_DEV=""
for p in /proc/asound/card$CARD/pcm*p; do
    [ -r "$p/info" ] || continue
    k=$(awk '/^id: *HDMI /{print $3; exit}' "$p/info" 2>/dev/null)
    [ -n "$k" ] || continue
    if [ "$(awk '/^monitor_present/{print $2}' "/proc/asound/card$CARD/eld#0.$k" 2>/dev/null)" = "1" ]; then
        PIN_DEV=$(basename "$p" | sed 's/^pcm//; s/p$//')
        break
    fi
done
[ -n "$PIN_DEV" ] || { echo "error: no HDMI pin reports monitor_present=1 — is the panel asleep?" >&2; exit 1; }
echo "panel on hw:$CARD,$PIN_DEV, probing DIG$DIG"
echo

TONE_PID=""
restore() {
    # UNCONDITIONAL restore — the whole point is that this leaves nothing behind.
    "$DUMP" --sample-send "$DIG:1" --fifo-reset-bit "$DIG:1" >/dev/null 2>&1 || true
    [ -n "$TONE_PID" ] && kill "$TONE_PID" 2>/dev/null
    if [ -n "${SUDO_UID:-}" ]; then chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$OUT" 2>/dev/null || true; fi
}
trap restore EXIT INT TERM

taps() {
    for s in 0 1; do
        v=$("$DUMP" --crc "$DIG" --crc-source "$s" --crc-count 2048 2>/dev/null | grep -E "CRC_DONE=" | head -1)
        echo "    source=$s :  ${v:-<no result>}"
    done
    st=$("$DUMP" 2>/dev/null | grep -E "^\s+DIG${DIG}_AFMT_STATUS\b" | head -1)
    echo "  ${st:-  AFMT_STATUS: <not read>}"
}

echo "Starting a 440 Hz tone to hw:$CARD,$PIN_DEV (-ac 2 mandatory: lavfi is mono and HDMI refuses 1ch)..."
ffmpeg -hide_banner -loglevel error -f lavfi -i "sine=frequency=440:duration=120:sample_rate=48000" \
    -af "volume=0.30" -ac 2 -ar 48000 -f alsa "hw:$CARD,$PIN_DEV" >/dev/null 2>&1 &
TONE_PID=$!
sleep 3
kill -0 $TONE_PID 2>/dev/null || { echo "error: ALSA refused the stream" >&2; exit 1; }

echo
echo "=== BASELINE: tone playing, UNMUTED (should be audible) ==="
taps
echo

echo "=== CASE D1: MUTED via AFMT_AUDIO_SAMPLE_SEND=0, bit11 (FIFO reset) LEFT SET ==="
echo "  (input still flowing: HDA DMA runs, LPIB advances, the codec keeps feeding the AFMT)"
"$DUMP" --sample-send "$DIG:0" 2>&1 | grep -E "AFMT_AUDIO_PACKET_CONTROL@" | sed 's/^/  /'
sleep 1
taps
echo

echo "=== CASE D2: MUTED, and bit11 AFMT_RESET_FIFO_WHEN_AUDIO_DIS ALSO CLEARED (confound control) ==="
echo "  (agnos's FIFO is OVERFLOWING, i.e. NOT in reset — this is the case that matches agnos's state)"
"$DUMP" --fifo-reset-bit "$DIG:0" 2>&1 | grep -E "AFMT_AUDIO_PACKET_CONTROL@" | sed 's/^/  /'
sleep 1
taps
echo

echo "=== RESTORING (tone should return) ==="
"$DUMP" --sample-send "$DIG:1" --fifo-reset-bit "$DIG:1" 2>&1 | grep -E "AFMT_AUDIO_PACKET_CONTROL@" | sed 's/^/  /'
sleep 1
taps
echo

cat <<'EOF'
=== HOW TO READ THIS ===

Compare the MUTED cases (D1/D2) against agnos's own reading:
      agnos:  source=0 -> DONE=1 CRC=0x25EB75 (content)   source=1 -> DONE=1 CRC=0x000000 (zero)

  source=1 -> DONE=0 (never completes) while muted
      -> MODEL A. The counter is gated per-tap. The instrument is VINDICATED, "zeros traverse" STANDS,
         agnos's bisection survives, and the hunt continues: something really is zeroing real samples
         inside the AFMT while every control register matches amdgpu.

  source=1 -> DONE=1, CRC=0x000000 while muted
      -> MODEL B. AGNOS'S EXACT SIGNATURE, reproduced on hardware that is merely MUTED and is definitely
         NOT passing 2048 zero samples.
      -> "CRC=0 means zeros flowed" is FALSIFIED BY DEMONSTRATION.
      -> agnos's AFMT output stage NEVER FIRES. Nothing zeroes anything; samples never leave.
      -> The target becomes: WHY DOES THE AUDIO-SAMPLE-PACKET INSERTER NEVER FIRE? — and note agnos's
         AFMT_AUDIO_SAMPLE_SEND already reads 1, byte-identical to amdgpu.

  BONUS — AFMT_STATUS bit24 (FIFO_OVERFLOW) under mute:
      If it SETS, the good-path mute reproduces agnos's ENTIRE signature (content at tap0 + DONE=1/CRC=0 at
      tap1 + overflow). The overflow stops being a symptom and becomes the DIAGNOSIS: fill outpaces a drain
      that is not running.

  D1 vs D2: if source=0 ALSO drops to DONE=0 in D1 but not D2, the bit11 FIFO reset confounded D1 — trust
  D2, which matches agnos's actual state (overflowing FIFO = not in reset).
EOF