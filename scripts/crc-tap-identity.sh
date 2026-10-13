#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# crc-tap-identity.sh — pin what AFMT_AUDIO_CRC_SOURCE actually measures, on the KNOWN-GOOD path.
#
# THE PROBLEM THIS SOLVES
#   agnos armed the AFMT audio CRC on its own (silent) path and read:
#       tap 0 -> CRC 0x25EB75  (non-zero)      tap 1 -> CRC 0x000000  (zero)
#   and concluded "samples reach the encoder and are zeroed between the taps". That conclusion assumes the
#   two taps are two SERIAL POINTS ON ONE SAMPLE PATH. **Nothing establishes that.**
#   AFMT_AUDIO_CRC_SOURCE is a bare 1-bit field (dcn_2_1_0_sh_mask.h shift 0x8, mask 0x100) with no enum,
#   no prose, and NO CONSUMER anywhere in the kernel tree — the only in-tree writes are legacy DCE parking
#   it disabled (dce_v8_0.c:1553, dce_v10_0.c:1601, dce_v11_0.c:1650, all 0x1000).
#
#   Two readings fit all four measured CRCs, and they point in OPPOSITE directions:
#     (a) SERIAL TAPS. Tap 0 = upstream, tap 1 = downstream. agnos has real samples at tap 0 and something
#         zeroes them before tap 1. => the whole Azalia endpoint diff (CONVERTER_FORMAT, AUDIO_DESCRIPTOR0,
#         SINK_INFO0, MULTICHANNEL_MODE, verb 0x789) is EXONERATED — it is all upstream of both taps.
#     (b) ONE TAP READS FRAMING. If a tap views the IEC-60958 subframe (preamble / channel status /
#         validity / parity) rather than raw PCM, its CRC is NON-ZERO EVEN OVER DIGITAL SILENCE, because
#         AFMT_60958_0/1/2 are programmed non-zero and are byte-identical on both paths. Then agnos's
#         "content at tap 0" is framing bits over silence, the samples are zero AT THE SOURCE, nothing is
#         happening between the taps, and the endpoint/converter/DMA path is fully back in scope.
#
#   Fifteen burns have been spent on inferred instrument semantics. This settles it with ZERO burns.
#
# THE EXPERIMENT
#   Arm the identical probe on the AUDIBLY-WORKING amdgpu path, twice: once with DIGITAL SILENCE playing,
#   once with a TONE. The stimulus is known; the instrument is the unknown.
#
#     tap 0 CRC CHANGES silence-vs-tone  => tap 0 is SAMPLE-CONTENT-sensitive
#                                        => agnos really does have real samples entering the AFMT
#                                        => reading (a): the zeroing IS between the taps, endpoint is dead,
#                                           hunt moves INSIDE the AFMT.
#     tap 0 CRC INVARIANT (non-zero both) => tap 0 reads FRAMING, not samples
#                                        => agnos's "content at tap 0" was an ECHO
#                                        => reading (b): samples are zero from the codec side; every
#                                           endpoint register agnos never writes is live again.
#
#   Either outcome kills half the search space and costs nothing.
#
# USAGE
#   Wake the display first (the ELD and audio pin presence vanish when the panel sleeps).
#   sudo ./scripts/crc-tap-identity.sh
#
#   Writes crc-tap-identity-<date>.txt in the repo root AND prints to the terminal — no piping needed.
#   Override with --out=PATH.
#
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
DUMP="$REPO/scripts/dump-dcn-audio.py"
CARD=0
DIG=1          # the live encoder on archaemenid
SECS=12
OUT="$REPO/crc-tap-identity-$(date +%Y-%m-%d-%H%M).txt"

for a in "$@"; do
    case "$a" in
        --out=*) OUT="${a#--out=}" ;;
        *) echo "unknown option: $a" >&2; exit 2 ;;
    esac
done

# Log everything to the file AND the terminal. This script asks the operator nothing, so unlike
# capture-hdmi-audio-known-good.sh there is no prompt for a redirect to swallow.
exec > >(tee "$OUT") 2>&1
# Hand the log back to the repo owner — we are running under sudo.
trap 'if [ -n "${SUDO_UID:-}" ]; then chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$OUT" 2>/dev/null || true; fi' EXIT

[ "$(id -u)" -eq 0 ] || { echo "error: needs root to write AFMT_AUDIO_CRC_CONTROL — re-run with sudo" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "error: ffmpeg not found" >&2; exit 1; }
[ -x "$DUMP" ] || { echo "error: $DUMP missing" >&2; exit 1; }

# --- resolve the pin the panel is on (device numbers are NOT sequential; derive, never hardcode)
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

# $1 = label, $2 = ffmpeg -i source spec
run_case() {
    label="$1"; src="$2"
    # -ac 2 is MANDATORY: lavfi sources are mono and an HDMI pin refuses a 1-channel stream outright
    # ("cannot set channel count to 1"), and ffmpeg dies before a sample moves.
    ffmpeg -hide_banner -loglevel error -f lavfi -i "$src" -ac 2 -ar 48000 -f alsa "hw:$CARD,$PIN_DEV" \
        >/dev/null 2>&1 &
    tp=$!
    sleep 3
    kill -0 $tp 2>/dev/null || { echo "  $label: ALSA refused the stream — cannot test" >&2; return 1; }
    for s in 0 1; do
        v=$("$DUMP" --crc "$DIG" --crc-source "$s" --crc-count 2048 2>/dev/null \
            | grep -E "CRC_DONE=|CRC_DONE=0" | head -1)
        echo "  $label  source=$s :  ${v:-<no result>}"
    done
    kill $tp 2>/dev/null || true; wait $tp 2>/dev/null || true
}

echo "=== CASE A: DIGITAL SILENCE playing (anullsrc — real stream, all-zero PCM) ==="
run_case "silence" "anullsrc=r=48000:cl=stereo" || true
echo
echo "=== CASE B: 440 Hz TONE playing ==="
run_case "tone   " "sine=frequency=440:duration=$SECS:sample_rate=48000" || true
echo

# --- CASE C: THE NULL. The calibration the first run of this script MISSED. -------------------------
# Cases A and B both have a LIVE STREAM. They prove the taps are content-SENSITIVE (zeros in -> CRC 0,
# tone in -> CRC non-zero) — but they do NOT establish the instrument's response to NO SAMPLES AT ALL.
# So "CRC_DONE=1, CRC=0x000000" still carries two meanings and only one has been measured:
#     (i)  2048 ZERO samples traversed          <- what agnos's tap-1 reading was taken to mean
#     (ii) the counter completed and NOTHING traversed   <- never tested
# If (ii), then agnos's tap 1 does not mean "the AFMT zeroes samples", it means "the AFMT read side never
# drains" — the paradox (every control register byte-identical, yet audio zeroed) dissolves, no register
# zeroes anything, and AFMT_STATUS bit24 FIFO_OVERFLOW becomes the centerpiece rather than an anomaly.
#
# dump-dcn-audio.py's own comment asserts "CRC_DONE asserts only when CRC_COUNT samples have physically
# traversed the AFMT". That is a COMMENT, not a source-derived fact: AFMT_AUDIO_CRC_COUNT is a bare 16-bit
# field with no in-tree consumer, and nothing in dcn_2_1_0_sh_mask.h says it counts VALID SAMPLES rather
# than SAMPLE CLOCKS. This case settles it.
echo "=== CASE C: THE NULL — nothing playing at all (the calibration that was missing) ==="
echo "  (also reading AFMT_CNTL: if amdgpu tore the audio clock down on close, this case is inconclusive)"
sleep 2
for s in 0 1; do
    v=$("$DUMP" --crc "$DIG" --crc-source "$s" --crc-count 2048 2>/dev/null | grep -E "CRC_DONE=" | head -1)
    echo "  null     source=$s :  ${v:-<no result>}"
done
acl=$("$DUMP" 2>/dev/null | grep -E "^\s+DIG${DIG}_AFMT_CNTL\b" | head -1)
echo "  ${acl:-  AFMT_CNTL: <not read>}"
echo
cat <<'EOF'
=== HOW TO READ THIS ===

A vs B — CONTENT SENSITIVITY (settled 2026-07-15: both taps ARE content-sensitive)
  tap CRC differs between silence and tone  -> that tap reads PCM.
  tap CRC identical and non-zero for both   -> that tap reads IEC-60958 FRAMING, not PCM.

C — THE NULL. THIS IS THE ONE THAT MATTERS NOW.
  Read AFMT_CNTL first. If it dropped to 0x0, amdgpu tore the audio clock down on stream close and this
  case is INCONCLUSIVE by this route — retry with SIGSTOP on the ffmpeg process instead of killing it, and
  even then treat it cautiously (ALSA may zero-fill on underrun, which reproduces the ambiguity rather
  than resolving it). With AFMT_CNTL still 0x101:

  CRC_DONE=1, CRC=0x000000 with NOTHING PLAYING
      -> A ZERO CRC DOES NOT MEAN DIGITAL SILENCE. It means NOTHING TRAVERSED.
      -> agnos's bisection is INVALID. "Real samples are zeroed inside the AFMT" is RETRACTED.
      -> The hunt becomes: WHY DOES THE AFMT READ SIDE NOT DRAIN?
      -> AFMT_STATUS bit24 FIFO_OVERFLOW (agnos set / amdgpu clear) is restored as the PRIMARY evidence,
         and its "wrong sign" refutation is VOID — that objection assumed zeros were provably traversing.

  CRC_DONE=0 with NOTHING PLAYING
      -> The counter genuinely requires samples to traverse. A zero CRC really does mean 2048 zero samples.
      -> agnos's bisection STANDS: real samples enter the AFMT and leave as digital silence.
      -> Then the cause is NOT register-visible (every AFMT control register is byte-identical to amdgpu),
         and the ATOM DIGxEncoderControl lane — the setup agnos's live DIG_MODE 2->3 flip never performs —
         is the only one left standing.

  Either way: the arc stops guessing about its own instrument.
EOF
