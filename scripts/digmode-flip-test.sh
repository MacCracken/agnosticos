#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# digmode-flip-test.sh — does the DIG_MODE flip DESTROY audio state? Fully automatic; no ears needed.
#
# WHAT THE LAST RUN REVEALED (and what my script was too clumsy to record)
#   The operator saw BLACKOUTS during the replay, with audio dying and returning. The script asked "still
#   audible?" only AFTER a 2-second sleep — by which time it had recovered — so it logged four clean passes
#   and MISSED the entire signal. The transient WAS the data.
#
#   A blackout means a write RE-LATCHED THE LINK. The only candidate in that sequence is DIG_BE_CNTL: even
#   writing DIG_MODE 3->3 re-latches the encoder and blips the link.
#
# WHY THIS IS THE WHOLE BALLGAME
#   agnos does:   HDMI_CONTROL, HDMI_GC, HDMI_VBI  ->  DIG_MODE 2->3 (a REAL mode change)  ->  audio setup
#   amdgpu does:  DIG_MODE     ->  HDMI_CONTROL ... HDMI_GC  ->  audio setup
#   agnos programs the HDMI packet block BEFORE the flip, and programs audio immediately AFTER it with NO
#   settle wait. If the flip re-latches or resets audio-side state, every agnos write still lands as a
#   VALUE — which is why all 51 registers read byte-identical to a working amdgpu — while the hardware
#   latched wrong internal state. That is EXACTLY agnos's signature: real samples in (CRC tap0 = content),
#   digital silence out (tap1 = 0), every control register correct.
#
#   The register-VALUE theory is already dead by demonstration (all six differences forced onto the working
#   path: harmless; the RAMP block: inert zeros). ORDER/TIMING is what is left, and this tests it directly.
#
# THE TEST (automatic — samples the CRC taps as fast as possible around each write, catching transients)
#   T1  DIG_MODE 3->3 rewrite. Does a same-value write alone re-latch and disturb audio?
#   T2  DIG_MODE 3->2->3 — what agnos ACTUALLY does (the GOP leaves this link at 2/DVI; agnos flips to 3).
#       Then WITHOUT reprogramming anything, watch whether audio returns on its own.
#       *** If audio does NOT return, the flip DESTROYS audio state and agnos must (re)program the audio
#           path AFTER it has settled. That is the bug and the fix. ***
#
# SAFETY / HONESTY
#   T2 sets DIG_MODE to 2 (DVI) briefly. THE SCREEN WILL LIKELY BLACK OUT for ~2 seconds and come back —
#   the operator already observed exactly this, recovering each time. Everything is restored unconditionally
#   on exit including Ctrl-C, and amdgpu re-establishes the link on any mode set / replug. Do not run this
#   while you need the screen. Pass --no-t2 to skip the risky half and only run T1.
#
# USAGE: sudo ./scripts/digmode-flip-test.sh [--no-t2]
#
set -eu
REPO="$(cd "$(dirname "$0")/.." && pwd)"
D="$REPO/scripts/dump-dcn-audio.py"
CARD=0; DIG=1
DIG_BE=$((0x20AF + 0x100))
RUN_T2=1
OUT="$REPO/digmode-flip-$(date +%Y-%m-%d-%H%M%S).txt"
for a in "$@"; do case "$a" in --no-t2) RUN_T2=0;; --out=*) OUT="${a#--out=}";; *) echo "unknown: $a" >&2; exit 2;; esac; done
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

O_BE=$("$D" 2>/dev/null | awk '/DIG1_DIG_BE_CNTL/{print $3}')
TONE_PID=""
restore() {
    "$D" --reg-write "2:$DIG_BE:$O_BE" >/dev/null 2>&1 || true
    [ -n "$TONE_PID" ] && kill "$TONE_PID" 2>/dev/null
    [ -n "${SUDO_UID:-}" ] && chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$OUT" 2>/dev/null || true
}
trap restore EXIT INT TERM

echo "panel hw:$CARD,$PIN_DEV — DIG$DIG   DIG_BE_CNTL original = $O_BE"
echo "Starting a continuous 440 Hz tone."
ffmpeg -hide_banner -loglevel error -f lavfi -i "sine=frequency=440:duration=900:sample_rate=48000" \
    -af "volume=0.30" -ac 2 -ar 48000 -f alsa "hw:$CARD,$PIN_DEV" >/dev/null 2>&1 &
TONE_PID=$!
sleep 3

# ONE CRC arm on tap1 = the fast oracle. CRC != 0  => real samples egressing. CRC == 0 => SILENCE.
# DONE == 0 => no flow at all. This is what agnos reads as 0x000000 while amdgpu reads content.
probe() {
    r=$("$D" --crc "$DIG" --crc-source 1 --crc-count 512 2>/dev/null | grep -oE "CRC_DONE=[01]  CRC=0x[0-9a-f]+|CRC_DONE=0" | head -1)
    printf "    %-8s tap1: %s\n" "$1" "${r:-<none>}"
}

echo; echo "=== BASELINE (tone playing) ==="; probe "t=0"; probe "t=1"

echo
echo "================================================================"
echo " T1 — DIG_MODE 3->3 rewrite (same value). Does the WRITE alone re-latch?"
echo "================================================================"
"$D" --reg-write "2:$DIG_BE:$O_BE" 2>&1 | grep wrote | sed 's/^/  /'
# sample IMMEDIATELY and repeatedly — this is what the last script missed
for i in 1 2 3 4 5 6; do probe "n=$i"; done
echo "  (if any tap1 shows CRC=0x000000 or DONE=0, the same-value write disturbed audio)"

if [ "$RUN_T2" -eq 1 ]; then
    BE2=$(printf "0x%08x" $(( $(printf "%d" "$O_BE") & ~0x00070000 | 0x00020000 )))   # DIG_MODE [18:16] = 2
    echo
    echo "================================================================"
    echo " T2 — DIG_MODE 3 -> 2 (DVI) -> 3, exactly what agnos does at boot."
    echo "      THE SCREEN MAY BLACK OUT for a couple of seconds. It comes back."
    echo "================================================================"
    echo "  --- going to DVI (DIG_MODE=2, $BE2) ---"
    "$D" --reg-write "2:$DIG_BE:$BE2" 2>&1 | grep wrote | sed 's/^/  /'
    for i in 1 2 3; do probe "dvi$i"; done
    echo "  --- back to HDMI (DIG_MODE=3, $O_BE) — and then NOTHING is reprogrammed ---"
    "$D" --reg-write "2:$DIG_BE:$O_BE" 2>&1 | grep wrote | sed 's/^/  /'
    for i in 1 2 3 4 5 6 7 8; do probe "back$i"; done
    echo
    echo "  *** THE QUESTION: did tap1 return to CONTENT on its own, with NO reprogramming? ***"
fi

echo; echo "=== RESTORED ==="; "$D" --reg-write "2:$DIG_BE:$O_BE" >/dev/null 2>&1; sleep 2
probe "final"
echo
cat <<'EOF'
=== HOW TO READ THIS ===
  T2 "back" probes show tap1 = CRC 0x000000 and it STAYS zero
      -> THE MODE FLIP DESTROYS AUDIO STATE. agnos's whole audio setup happens right after exactly this
         flip, so it is programming a block that the flip has just torn down. Every value lands, the
         registers read perfect, and the hardware stays silent. THAT IS AGNOS'S BUG.
      -> FIX: after the DIG_MODE flip, wait for the link to settle and THEN program the audio path —
         and move agnos's HDMI_CONTROL/HDMI_GC/HDMI_VBI writes to AFTER the flip, matching amdgpu's order.

  T2 "back" probes return to CONTENT on their own
      -> the flip is survivable and self-healing; ordering around it is NOT the bug, and the arc needs a
         different lead entirely.

  T1 disturbs tap1 at all
      -> even a same-value DIG_BE_CNTL write re-latches. agnos does a REAL 2->3 change, so it gets at least
         this much disturbance and probably more.
EOF