#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# perturb-toward-agnos.sh — break the WORKING amdgpu path toward agnos, one register at a time, and find
#                           which single value kills the sound. YOUR EARS ARE THE ORACLE.
#
# WHY THIS EXISTS
#   Sixteen burns and four adversarial audits have all COMPARED STATES and then argued about which of the
#   differences "could" matter. That is theory, and it has produced ~30 refuted hypotheses and zero sound.
#   Every one of those refutations rests on a chain of inference (what the CRC taps observe, what is
#   "upstream", what "cannot" zero samples).
#
#   This is the inverse and it is a MEASUREMENT: the amdgpu path is audibly working on this exact panel.
#   Apply ONE of agnos's values to it. Listen. If the tone dies, that value IS the bug — no inference
#   required, and any "it's exonerated / it's upstream / it can't matter" argument is refuted by
#   demonstration.
#
#   If NONE of them kills the sound, that is also a real result: it means agnos's silence is not caused by
#   any register value we have identified as different, and the difference is somewhere we have not looked.
#
# WHAT IT PERTURBS (every measured agnos-vs-amdgpu difference, including the "exonerated" ones —
# the exonerations are exactly what is being tested)
#   1. CONVERTER_FORMAT   ix0x02  amdgpu 0x31 (24-bit) -> agnos 0x11 (16-bit)
#   2. CHANNEL_STREAM_ID  ix0x03  amdgpu 0x30 (tag 3)  -> agnos 0x10 (tag 1)
#   3. MULTICHANNEL_MODE  ix0x58  amdgpu 0x1           -> agnos 0x0
#   4. AUDIO_DESCRIPTOR0  ix0x28  amdgpu 0x07070701    -> agnos 0x07010701
#   5. SINK_INFO0         ix0x3a  amdgpu 0x07757204    -> agnos 0x0
#   6. DTO0_MODULE  BASE_IDX1 0x00AD  amdgpu 0x24D998  -> agnos 0x24D9B8
#
# SAFETY
#   Every perturbation is restored before the next one, and all originals are restored on exit including on
#   Ctrl-C. These are audio-path registers only: no modeset, no link change, no PHY, nothing that can
#   black-screen the console. Worst case is silence until restore, and a replug or mode change would reset
#   them anyway.
#
# USAGE
#   Wake the display. Put the volume somewhere you can clearly hear the tone.
#   sudo ./scripts/perturb-toward-agnos.sh
#   Logs to perturb-<date>.txt and prints to the terminal.
#
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
DUMP="$REPO/scripts/dump-dcn-audio.py"
CARD=0; DIG=1; EP=1
OUT="$REPO/perturb-$(date +%Y-%m-%d-%H%M).txt"
for a in "$@"; do case "$a" in --out=*) OUT="${a#--out=}";; *) echo "unknown: $a" >&2; exit 2;; esac; done

[ "$(id -u)" -eq 0 ] || { echo "error: needs root — re-run with sudo" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "error: ffmpeg not found" >&2; exit 1; }

exec > >(tee "$OUT") 2>&1

PIN_DEV=""
for p in /proc/asound/card$CARD/pcm*p; do
    [ -r "$p/info" ] || continue
    k=$(awk '/^id: *HDMI /{print $3; exit}' "$p/info" 2>/dev/null); [ -n "$k" ] || continue
    if [ "$(awk '/^monitor_present/{print $2}' "/proc/asound/card$CARD/eld#0.$k" 2>/dev/null)" = "1" ]; then
        PIN_DEV=$(basename "$p" | sed 's/^pcm//; s/p$//'); break
    fi
done
[ -n "$PIN_DEV" ] || { echo "error: no HDMI pin has monitor_present=1 — panel asleep?" >&2; exit 1; }

TONE_PID=""
restore_all() {
    "$DUMP" --az-write "$EP:0x02:0x31" --az-write "$EP:0x03:0x30" --az-write "$EP:0x58:0x1" \
            --az-write "$EP:0x28:0x07070701" --az-write "$EP:0x3a:0x07757204" \
            --reg-write "1:0x00AD:0x24D998" >/dev/null 2>&1 || true
    [ -n "$TONE_PID" ] && kill "$TONE_PID" 2>/dev/null
    [ -n "${SUDO_UID:-}" ] && chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$OUT" 2>/dev/null || true
}
trap restore_all EXIT INT TERM

echo "panel on hw:$CARD,$PIN_DEV — DIG$DIG, AZ endpoint $EP"
echo "Starting a continuous 440 Hz tone. IT SHOULD BE AUDIBLE NOW."
ffmpeg -hide_banner -loglevel error -f lavfi -i "sine=frequency=440:duration=600:sample_rate=48000" \
    -af "volume=0.30" -ac 2 -ar 48000 -f alsa "hw:$CARD,$PIN_DEV" >/dev/null 2>&1 &
TONE_PID=$!
sleep 3
kill -0 $TONE_PID 2>/dev/null || { echo "error: ALSA refused the stream" >&2; exit 1; }

taps() {
    for s in 0 1; do
        v=$("$DUMP" --crc "$DIG" --crc-source "$s" --crc-count 2048 2>/dev/null | grep -E "CRC_DONE=" | head -1)
        echo "      tap$s: ${v:-<none>}"
    done
}

ask() {
    printf "\n  >>> Is the tone STILL AUDIBLE? [y = still hear it / n = IT DIED] > " >/dev/tty
    read -r ans </dev/tty || ans="?"
    case "$ans" in
        n|N) echo "  *** THE TONE DIED. THIS VALUE IS THE BUG. ***" ;;
        y|Y) echo "  (still audible — this value is NOT the cause)" ;;
        *)   echo "  (unanswered)" ;;
    esac
}

# $1 label, $2.. the dump args to apply, then the restore args are handled per-case
perturb() {
    label="$1"; shift
    echo
    echo "================================================================"
    echo " PERTURBATION: $label"
    echo "================================================================"
    "$DUMP" "$@" 2>&1 | grep -E "wrote|==" | sed 's/^/  /'
    sleep 2
    taps
    ask
}

echo
echo "=== BASELINE (unperturbed, should be audible) ==="; taps

perturb "CONVERTER_FORMAT 0x31 (24-bit) -> 0x11 (agnos: 16-bit)"  --az-write "$EP:0x02:0x11"
"$DUMP" --az-write "$EP:0x02:0x31" >/dev/null 2>&1; sleep 1

perturb "CHANNEL_STREAM_ID 0x30 (tag 3) -> 0x10 (agnos: tag 1)"   --az-write "$EP:0x03:0x10"
"$DUMP" --az-write "$EP:0x03:0x30" >/dev/null 2>&1; sleep 1

perturb "MULTICHANNEL_MODE 0x1 -> 0x0 (agnos: PAIRED)"            --az-write "$EP:0x58:0x0"
"$DUMP" --az-write "$EP:0x58:0x1" >/dev/null 2>&1; sleep 1

perturb "AUDIO_DESCRIPTOR0 0x07070701 -> 0x07010701 (agnos)"      --az-write "$EP:0x28:0x07010701"
"$DUMP" --az-write "$EP:0x28:0x07070701" >/dev/null 2>&1; sleep 1

perturb "SINK_INFO0 0x07757204 -> 0x0 (agnos: never written)"     --az-write "$EP:0x3a:0x0"
"$DUMP" --az-write "$EP:0x3a:0x07757204" >/dev/null 2>&1; sleep 1

perturb "DCCG_AUDIO_DTO0_MODULE 0x24D998 -> 0x24D9B8 (agnos)"     --reg-write "1:0x00AD:0x24D9B8"
"$DUMP" --reg-write "1:0x00AD:0x24D998" >/dev/null 2>&1; sleep 1

echo
echo "=== ALL RESTORED — the tone should be audible again ==="; taps
echo
cat <<'EOF'
=== HOW TO READ THIS ===
  ANY perturbation that KILLED the tone IS THE BUG, by demonstration. It refutes every argument that the
  value is "upstream", "exonerated", "informational", or "cannot matter" — those were inferences; this is
  the working hardware answering.

  If NONE killed it: every measured agnos-vs-amdgpu register difference is genuinely harmless, and agnos's
  silence is caused by something NOT in the diff — i.e. not a register value at all. That is a real result
  too: it closes the "wrong value somewhere" theory completely and forces the search onto sequence/state.

  Watch the taps as well as your ears: a perturbation that drives tap1 to CRC=0x000000 reproduces agnos's
  exact signature, which is the strongest possible match even if the tone survives.
EOF