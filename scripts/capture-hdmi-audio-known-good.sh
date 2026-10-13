#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# capture-hdmi-audio-known-good.sh — capture the COMPLETE known-good HDMI-audio state
# off a live amdgpu on archaemenid, while sound is actually playing, and store it as
# prior-art.
#
# WHY THIS EXISTS
#   The agnos display-audio arc has burned twelve times against a known-good capture that
#   was partial and never stored — it was taken to settle specific disputes and then only
#   quoted in a changelog. Every session since has re-derived it. This script captures it
#   whole, once, into a committed file, the same way
#   docs/development/prior-art/r8169-live-linux-register-dump-2026-05-25.md did for the NIC.
#
# WHAT IT CAPTURES (both halves of the path — the BAR5 dump alone cannot see the codec)
#   1. The DCN side  — read-only BAR5 mmap of 04:00.0 via dump-dcn-audio.py (all DIGs,
#                      all Azalia endpoints, bitfield-decoded).
#   2. The codec side — /proc/asound/card0/codec#0, the ATI HDMI codec's full widget state,
#                      plus every ELD.
#   Both taken WHILE A TONE IS PLAYING, which is the only state that is "known-good".
#
# THE PREMISE CHECK (do not skip)
#   Twelve burns have assumed this monitor emits sound over HDMI. Nobody has verified it.
#   This script plays a tone and ASKS. If you cannot hear it under Linux + amdgpu, agnos
#   was never going to make a sound and the arc has been chasing a bug that isn't there.
#
# SAFETY
#   dump-dcn-audio.py opens BAR5 O_RDONLY / PROT_READ in its default mode — a write is not
#   merely unattempted, it is impossible. --az is different: it writes the Azalia INDEX
#   register to steer the index/data window. That races amdgpu, which also uses that window.
#   It has been done before on this box without incident, but it is the one non-read-only
#   part of this capture. Pass --no-az to skip it.
#
# USAGE
#   1. Make sure the monitor is AWAKE (the ELD and the audio pin presence disappear when the
#      panel sleeps — this script will refuse if no pin reports a monitor).
#   2. sudo ./scripts/capture-hdmi-audio-known-good.sh
#
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$REPO/docs/development/prior-art/dcn-audio-live-linux-register-dump-$(date +%Y-%m-%d).md"
DUMP="$REPO/scripts/dump-dcn-audio.py"
CARD=0
TONE_HZ=440
TONE_SECS=45
TONE_VOL=0.30   # a full-scale sine is unpleasant to sit next to for 45 s; 0.30 is plainly audible
AZ_FLAG="--az"

for a in "$@"; do
    case "$a" in
        --no-az) AZ_FLAG="" ;;
        --out=*) OUT="${a#--out=}" ;;
        *) echo "unknown option: $a" >&2; exit 2 ;;
    esac
done

if [ "$(id -u)" -ne 0 ]; then
    echo "error: needs root to mmap BAR5 — re-run with sudo" >&2
    exit 1
fi
[ -x "$DUMP" ] || { echo "error: $DUMP not found/executable" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "error: ffmpeg not found (needed to play the tone straight to ALSA)" >&2; exit 1; }

# --- Which pin has the panel? The ELD is the ground truth, and it is DYNAMIC: it vanishes
#     when the monitor sleeps.
#
#     The ALSA device numbers are NOT sequential and NOT the pin nid: on this codec the four
#     digital pins 0x03/0x05/0x07/0x09 land on hw:0,3 / hw:0,7 / hw:0,8 / hw:0,9. Rather than
#     hardcode that, derive it: each pcmNp/info carries `id: HDMI <k>`, and that <k> is the
#     same ordinal as eld#0.<k>. So HDMI 1 == eld#0.1 == pin 0x05 == hw:0,7.
PIN_DEV=""
PIN_ELD=""
for p in /proc/asound/card$CARD/pcm*p; do
    [ -r "$p/info" ] || continue
    k=$(awk '/^id: *HDMI /{print $3; exit}' "$p/info" 2>/dev/null)
    [ -n "$k" ] || continue
    dev=$(basename "$p" | sed 's/^pcm//; s/p$//')
    eld="/proc/asound/card$CARD/eld#0.$k"
    if [ -r "$eld" ] && [ "$(awk '/^monitor_present/{print $2}' "$eld" 2>/dev/null)" = "1" ]; then
        PIN_DEV="$dev"; PIN_ELD="$eld"
        break
    fi
done

if [ -z "$PIN_DEV" ]; then
    echo "error: no HDMI pin reports monitor_present=1." >&2
    echo "       The panel is probably asleep — the ELD and the audio presence both go away" >&2
    echo "       with the link. Wake the display and re-run." >&2
    exit 1
fi

echo "Panel found on $PIN_ELD -> ALSA device hw:$CARD,$PIN_DEV"
awk '/^monitor_name|^codec_pin_nid|^codec_cvt_nid|^eld_valid/{print "  " $0}' "$PIN_ELD"
echo

# --- Play the tone straight to ALSA. PipeWire currently has no cards claimed on this box
#     (wpctl shows a Dummy Output only), so going through it would prove nothing.
#
#     -ac 2 IS LOAD-BEARING. ffmpeg's lavfi `sine` source emits MONO, and an HDMI audio pin will not accept
#     a 1-channel stream: ALSA rejects the open outright with "cannot set channel count to 1 (Invalid
#     argument)" and ffmpeg dies before a single sample moves. That is a bug in this script, not a finding
#     about the hardware — it cost one operator run. -ar 48000 matches what the sink advertises and what
#     agnos itself streams (HDA_FMT_48K_16_2); ALSA then negotiates S16_LE on its own, verified against
#     pcm7p/hw_params reading `format: S16_LE  channels: 2  rate: 48000` during a live tone.
echo "Playing a ${TONE_HZ} Hz sine to hw:$CARD,$PIN_DEV for ${TONE_SECS}s (stereo, 48 kHz) ..."
ffmpeg -hide_banner -loglevel error \
       -f lavfi -i "sine=frequency=$TONE_HZ:duration=$TONE_SECS:sample_rate=48000" \
       -af "volume=$TONE_VOL" -ac 2 -ar 48000 -f alsa "hw:$CARD,$PIN_DEV" &
TONE_PID=$!
trap 'kill $TONE_PID 2>/dev/null || true' EXIT INT TERM
sleep 3

if ! kill -0 $TONE_PID 2>/dev/null; then
    echo "error: the tone process died immediately — ALSA refused hw:$CARD,$PIN_DEV." >&2
    echo "       That is itself a finding: capture the error by running the ffmpeg line by hand." >&2
    exit 1
fi

# --- THE PREMISE CHECK. This is the whole reason the script stops here.
#     Everything here goes to /dev/tty, not stdout: this script is meant to be run with stdout redirected
#     to a file, and a question the operator cannot see is a question that does not get asked.
{
    echo
    echo "================================================================"
    echo " Can you HEAR the tone right now, out of the monitor?"
    echo
    echo " This matters more than the dump. Twelve agnos burns have assumed"
    echo " this panel emits sound over HDMI and none of them checked. If the"
    echo " answer is no, agnos was never going to make a sound and the whole"
    echo " arc has been chasing a bug that does not exist."
    echo "================================================================"
    printf " [y = I hear it / n = silence] > "
} >/dev/tty 2>&1
read -r HEARD </dev/tty || HEARD="?"

case "$HEARD" in
    y|Y) VERDICT="AUDIBLE — amdgpu plays sound out this panel over HDMI. The sink works, so agnos's silence is agnos's bug." ;;
    n|N) VERDICT="SILENT UNDER AMDGPU TOO — the reference driver cannot make this panel sound either. agnos's display-audio path was never going to produce a tone; the premise of the whole arc is wrong." ;;
    *)   VERDICT="UNANSWERED — the operator did not confirm. Treat every value below as unvalidated." ;;
esac
echo
echo "recorded: $VERDICT"
echo

# --- Capture both halves WHILE the tone plays.
mkdir -p "$(dirname "$OUT")"
{
    echo "---"
    echo "name: DCN 2.1 display-audio live Linux register dump"
    echo "description: known-good amdgpu HDMI-audio register state captured off archaemenid while a tone played, both DCN and codec sides"
    echo "type: prior-art"
    echo "---"
    echo
    echo "# DCN 2.1 Display-Audio — Live Linux (amdgpu) Register Dump"
    echo
    echo "Captured $(date -Iseconds) on archaemenid (AMD Cezanne, 1002:1638 / 1002:1637, DCN 2.1)"
    echo "by \`scripts/capture-hdmi-audio-known-good.sh\`, **while a ${TONE_HZ} Hz tone was playing**"
    echo "to \`hw:$CARD,$PIN_DEV\`. This is the state agnos's sovereign path must reproduce."
    echo
    echo "## Operator verdict — does the sink actually make sound?"
    echo
    echo "**$VERDICT**"
    echo
    echo "## Environment"
    echo
    echo '```'
    echo "kernel:  $(uname -r)"
    echo "amdgpu:  $(modinfo -F version amdgpu 2>/dev/null || echo 'in-tree')"
    echo "panel:   $(awk '/^monitor_name/{ $1=""; sub(/^ /,""); print }' "$PIN_ELD" 2>/dev/null)"
    echo "pin:     $(awk '/^codec_pin_nid/{print $2}' "$PIN_ELD" 2>/dev/null) (ALSA hw:$CARD,$PIN_DEV)"
    echo '```'
    echo
    echo "## ELD — every pin (the ground truth for WHICH pin has the panel; dynamic, vanishes when the panel sleeps)"
    echo
    echo '```'
    for e in /proc/asound/card$CARD/eld#*; do
        echo "--- $e"
        cat "$e"
    done
    echo '```'
    echo
    echo "## DCN side — BAR5 (04:00.0), read-only mmap"
    echo
    echo '```'
    # --force-az IS REQUIRED HERE. --az prompts "type 'yes' to proceed" and this whole block runs with its
    # stdin redirected, so the prompt reads EOF and aborts every time — silently producing a file titled
    # "known-good register dump" whose DCN section contains nothing but the abort message. That happened on
    # the 2026-07-15 08:22 run and the artifact looked plausible enough to file. The operator already
    # consented by running this script; re-prompting inside a redirect only breaks it.
    "$DUMP" $AZ_FLAG ${AZ_FLAG:+--force-az} 2>&1 || echo "(dump-dcn-audio.py exited non-zero)"
    echo '```'
    echo
    echo "## Codec side — the ATI HDMI codec (04:00.1), full widget state"
    echo
    echo "The BAR5 dump cannot see this half. Node 0x04/0x06/0x08/0x0a are the converters;"
    echo "0x03/0x05/0x07/0x09 are the pins. Look for the converter bound to a live stream"
    echo "(\`Converter: stream=N\` with N != 0) and \`Digital: Enabled\`."
    echo
    echo '```'
    cat "/proc/asound/card$CARD/codec#0"
    echo '```'
    echo
    echo "## PCM device state during playback"
    echo
    echo '```'
    for s in /proc/asound/card$CARD/pcm*/sub*/{status,hw_params}; do
        [ -r "$s" ] || continue
        echo "--- $s"
        cat "$s" 2>/dev/null
    done
    echo '```'
} > "$OUT" 2>&1

kill $TONE_PID 2>/dev/null || true
wait $TONE_PID 2>/dev/null || true
trap - EXIT INT TERM

# The file is written by root under sudo; hand it back to the repo owner.
if [ -n "${SUDO_UID:-}" ]; then
    chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$OUT" 2>/dev/null || true
fi

echo "Wrote $OUT"
echo
echo "Next: diff the DCN half against agnos's own \`gpu_audio_dump()\` output, which prints the"
echo "same registers in the same order. Build agnos with HDMI_AUDIO_DUMP=1 and capture with"
echo "\`run /bin/klug > dump.txt\`."
