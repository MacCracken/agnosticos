#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# poke-dcn-audio.py - kill-bisect AGNOS's DCN 2.1 display-audio register state against
# a LIVE, AUDIBLY-PLAYING amdgpu path, by ear, one block at a time.
#
# THE IDEA (operator's, 2026-07-16): instead of guessing one hypothesis per agnos reflash,
# use Linux as the test bench. amdgpu IS the working driver on this exact silicon. So:
#   1. play audio through the monitor under amdgpu (it is audible),
#   2. write AGNOS's register values (from the audio_re_6 iron dump) onto the live path,
#      one functional block at a time,
#   3. listen. The block that KILLS the sound is, deterministically, what agnos has wrong.
#      If NO block kills it, the fault is not in this register file at all — also decisive.
# Feedback per iteration: seconds, by ear, zero reflashes. This extends the 2026-07-15
# perturbation test (which proved live pokes are survivable) to the rows it never covered.
#
# Usage (Linux, amdgpu loaded, audio PLAYING to the monitor the whole time):
#   speaker-test -D plughw:0,7 -c 2 -t sine -f 440     # panel is hw:0,7 on archaemenid
#   sudo ./scripts/poke-dcn-audio.py --bisect          # guided block-by-block A/B
#   sudo ./scripts/poke-dcn-audio.py --read RAMP       # just read a block, no writes
#   sudo ./scripts/poke-dcn-audio.py --write AFMT_RAMP_CONTROL0=0 --dig 1
#
# Every block is restored to the captured original values immediately after the ear
# verdict, so each block is an independent A/B and nothing compounds. Reboot after the
# session anyway — amdgpu was not consulted about any of this.
#
# Offsets are the SAME table as dump-dcn-audio.py (dcn_2_1_0_offset.h, triple-sourced).

import argparse
import mmap
import os
import struct
import sys
import time

SEG = [0x00000012, 0x000000C0, 0x000034C0, 0x00009000, 0x02403C00]
DIG_STRIDE = 0x100


def addr(base_idx, off):
    return (SEG[base_idx] + off) * 4


# name -> (base_idx, DIG0 dword offset, dig_relative?)
REGS = {
    "DIG_FE_CNTL":                  (2, 0x2068, True),
    "HDMI_CONTROL":                 (2, 0x2071, True),
    "HDMI_STATUS":                  (2, 0x2072, True),
    "HDMI_AUDIO_PACKET_CONTROL":    (2, 0x2073, True),
    "HDMI_ACR_PACKET_CONTROL":      (2, 0x2074, True),
    "HDMI_VBI_PACKET_CONTROL":      (2, 0x2075, True),
    "HDMI_INFOFRAME_CONTROL0":      (2, 0x2076, True),
    "HDMI_INFOFRAME_CONTROL1":      (2, 0x2077, True),
    "HDMI_GENERIC_PACKET_CONTROL0": (2, 0x2078, True),
    "HDMI_GC":                      (2, 0x207B, True),
    "AFMT_AUDIO_PACKET_CONTROL2":   (2, 0x207C, True),
    "HDMI_DB_CONTROL":              (2, 0x2088, True),
    "AFMT_GENERIC_HDR":             (2, 0x208C, True),
    "AFMT_GENERIC_0":               (2, 0x208D, True),
    "AFMT_GENERIC_2":               (2, 0x208F, True),
    "AFMT_GENERIC_3":               (2, 0x2090, True),
    "HDMI_GENERIC_PACKET_CONTROL1": (2, 0x2095, True),
    "HDMI_ACR_48_0":                (2, 0x209A, True),
    "HDMI_ACR_48_1":                (2, 0x209B, True),
    "AFMT_60958_0":                 (2, 0x20A0, True),
    "AFMT_60958_1":                 (2, 0x20A1, True),
    "AFMT_RAMP_CONTROL0":           (2, 0x20A3, True),
    "AFMT_RAMP_CONTROL1":           (2, 0x20A4, True),
    "AFMT_RAMP_CONTROL2":           (2, 0x20A5, True),
    "AFMT_RAMP_CONTROL3":           (2, 0x20A6, True),
    "AFMT_60958_2":                 (2, 0x20A7, True),
    "AFMT_STATUS":                  (2, 0x20A9, True),
    "AFMT_AUDIO_PACKET_CONTROL":    (2, 0x20AA, True),
    "AFMT_VBI_PACKET_CONTROL":      (2, 0x20AB, True),
    "AFMT_INFOFRAME_CONTROL0":      (2, 0x20AC, True),
    "AFMT_AUDIO_SRC_CONTROL":       (2, 0x20AD, True),
    "DIG_BE_CNTL":                  (2, 0x20AF, True),
    "DIG_BE_EN_CNTL":               (2, 0x20B0, True),
    "AFMT_CNTL":                    (2, 0x20E6, True),
    "DCCG_AUDIO_DTO_SOURCE":        (1, 0x00AB, False),
    "DCCG_AUDIO_DTO0_PHASE":        (1, 0x00AC, False),
    "DCCG_AUDIO_DTO0_MODULE":       (1, 0x00AD, False),
}

# ---------------------------------------------------------------------------
# THE BISECT PLAN. agnos's values are VERBATIM from the audio_re_6.txt iron dump
# (AGNOS 1.55.20 + RAMP fix, DIG1, 2026-07-16) — the exact state that is silent
# on iron with the sink's amp provably armed (shutdown-release evidence).
# Order = suspicion. Each entry: (title, why, [(reg, agnos_value)], risky?)
# ---------------------------------------------------------------------------
PLAN = [
    ("RAMP-zeros (agnos PRE-fix state)",
     "All four RAMP regs read 0 on agnos for the whole arc. If zeroing them kills amdgpu's "
     "audio, the all-zero RAMP was the mute. If audio survives, RAMP-zero is NOT the mute.",
     [("AFMT_RAMP_CONTROL0", 0x0), ("AFMT_RAMP_CONTROL1", 0x0),
      ("AFMT_RAMP_CONTROL2", 0x0), ("AFMT_RAMP_CONTROL3", 0x0)], False),

    ("RAMP-radeon (agnos 1.55.20 fix values)",
     "The staged fix wrote r600-lineage ffffff/7fffff/1/1 — and the shutdown-release "
     "DISAPPEARED on iron. If THESE values kill amdgpu's audio, the fix made it worse "
     "and DCN2.1 ramp semantics differ from radeon-era.",
     [("AFMT_RAMP_CONTROL0", 0xFFFFFF), ("AFMT_RAMP_CONTROL1", 0x7FFFFF),
      ("AFMT_RAMP_CONTROL2", 0x1), ("AFMT_RAMP_CONTROL3", 0x1)], False),

    ("IEC-60958 channel status",
     "Wrong channel-status (e.g. the validity/PCM bits) makes a sink decode silence while "
     "accepting the stream — precisely the iron symptom.",
     [("AFMT_60958_0", 0x100000), ("AFMT_60958_1", 0x200000),
      ("AFMT_60958_2", 0x876543)], False),

    ("AFMT audio packet block",
     "The sample-send / channel-enable / source-select cluster.",
     [("AFMT_AUDIO_PACKET_CONTROL", 0x801), ("AFMT_AUDIO_PACKET_CONTROL2", 0x300),
      ("AFMT_AUDIO_SRC_CONTROL", 0x1), ("AFMT_INFOFRAME_CONTROL0", 0x0),
      ("AFMT_CNTL", 0x101)], False),

    ("HDMI packet/InfoFrame controls",
     "Send-enables, lines, ACR control, GC, DB — the packet scheduling cluster.",
     [("HDMI_AUDIO_PACKET_CONTROL", 0x30010), ("HDMI_ACR_PACKET_CONTROL", 0x11000),
      ("HDMI_VBI_PACKET_CONTROL", 0x31), ("HDMI_INFOFRAME_CONTROL0", 0x10),
      ("HDMI_INFOFRAME_CONTROL1", 0x200), ("HDMI_GENERIC_PACKET_CONTROL0", 0x3),
      ("HDMI_GENERIC_PACKET_CONTROL1", 0x2), ("HDMI_GC", 0x4),
      ("HDMI_DB_CONTROL", 0x1000), ("HDMI_ACR_48_1", 0x1800)], False),

    ("AVI InfoFrame payload (agnos's RGB AVI)",
     "agnos declares RGB, amdgpu YCbCr444. The SCREEN may tint wrong for the duration — "
     "that is expected and reverts; listen for whether the AUDIO survives the swap.",
     [("AFMT_GENERIC_HDR", 0xD0282), ("AFMT_GENERIC_0", 0x81ECF),
      ("AFMT_GENERIC_2", 0x5CA), ("AFMT_GENERIC_3", 0xAA1)], False),

    ("DCCG audio DTO",
     "The audio clock divider pair (phase/module) + source select.",
     [("DCCG_AUDIO_DTO_SOURCE", 0x0), ("DCCG_AUDIO_DTO0_PHASE", 0x3A980),
      ("DCCG_AUDIO_DTO0_MODULE", 0x24D9B8)], False),

    ("HDMI_CONTROL",
     "Keepout/scramble/deep-color cluster. agnos=0x10019.",
     [("HDMI_CONTROL", 0x10019)], False),

    ("DIG_FE_CNTL  ** RISKY — display may flicker **",
     "Front-end control incl. pixel-encoding + stereosync gate. agnos=0x1000100. "
     "Contains source-select bits for agnos's topology — the console may glitch; it restores.",
     [("DIG_FE_CNTL", 0x1000100)], True),
]

SETTLE_S = 1.0   # let the packet engines pick up the new values before the ear verdict


class Bar:
    def __init__(self, path, writable):
        flags = os.O_RDWR if writable else os.O_RDONLY
        self.fd = os.open(path, flags | os.O_SYNC)
        size = os.fstat(self.fd).st_size
        prot = mmap.PROT_READ | (mmap.PROT_WRITE if writable else 0)
        self.mm = mmap.mmap(self.fd, size, mmap.MAP_SHARED, prot)
        self.size = size

    def rd(self, byte_off):
        if byte_off + 4 > self.size:
            return None
        return struct.unpack_from("<I", self.mm, byte_off)[0]

    def wr(self, byte_off, val):
        struct.pack_into("<I", self.mm, byte_off, val)


def reg_addr(name, dig):
    base_idx, off, dig_rel = REGS[name]
    if dig_rel:
        off = off + dig * DIG_STRIDE
    return addr(base_idx, off)


def do_read(bar, names, dig):
    for n in names:
        v = bar.rd(reg_addr(n, dig))
        print(f"  {n:<32} 0x{v:08x}")


def ask(prompt, choices):
    while True:
        r = input(f"{prompt} [{'/'.join(choices)}] ").strip().lower()
        if r in choices:
            return r


def bisect(bar, dig):
    print("=" * 74)
    print(" DCN display-audio kill-bisect — agnos values onto the LIVE amdgpu path")
    print(f" DIG instance: {dig}.  Keep audio PLAYING the whole session.")
    print(" After each block the ORIGINALS are restored and you confirm audio is back,")
    print(" so every block is an independent A/B. Answers wanted: your ears only.")
    print("=" * 74)
    verdicts = []
    for title, why, writes, risky in PLAN:
        print(f"\n--- BLOCK: {title}")
        print(f"    {why}")
        orig = {}
        for name, _ in writes:
            orig[name] = bar.rd(reg_addr(name, dig))
        print("    current (amdgpu-live) values:")
        for name, agv in writes:
            marker = "  <- DIFFERS" if orig[name] != agv else "  (same as agnos)"
            print(f"      {name:<32} 0x{orig[name]:08x}   agnos: 0x{agv:08x}{marker}")
        if all(orig[n] == v for n, v in writes):
            print("    every value already identical — nothing to test, skipping.")
            verdicts.append((title, "identical"))
            continue
        if risky:
            if ask("    risky block — proceed?", ["y", "n"]) == "n":
                verdicts.append((title, "skipped"))
                continue
        r = ask("    write agnos's values now?", ["y", "s", "q"])  # yes / skip / quit
        if r == "q":
            break
        if r == "s":
            verdicts.append((title, "skipped"))
            continue
        for name, agv in writes:
            bar.wr(reg_addr(name, dig), agv)
        time.sleep(SETTLE_S)
        v = ask("    >>> LISTEN: is the audio still playing?", ["y", "n"])
        verdicts.append((title, "SURVIVED" if v == "y" else "*** KILLED ***"))
        for name, _ in writes:            # restore originals unconditionally
            bar.wr(reg_addr(name, dig), orig[name])
        time.sleep(SETTLE_S)
        back = ask("    originals restored — is audio back?", ["y", "n"])
        if back == "n":
            print("    !! audio did not return on restore — note it and reboot after the session.")
        if v == "n":
            print(f"\n    ##### '{title}' KILLED the working path — this is the block agnos has wrong. #####")
            if ask("    continue bisecting the remaining blocks anyway?", ["y", "n"]) == "n":
                break
    print("\n" + "=" * 74)
    print(" VERDICTS")
    for t, v in verdicts:
        print(f"  {v:<16} {t}")
    print(" If nothing killed: the fault is NOT in this register file -> codec/HDA half.")
    print(" Reboot before returning to normal use.")
    print("=" * 74)


# bit constants — verbatim from agnos kernel/core/gpu_regs.cyr
AVMUTE       = 0x1          # HDMI_GC bit0
DIG_ENABLE   = 0x1          # DIG_BE_EN_CNTL bit0
SYMCLK_BE_ON = 0x100        # DIG_BE_EN_CNTL bit8 (RO ack)
SAMPLE_SEND  = 0x1          # AFMT_AUDIO_PACKET_CONTROL bit0
OVERFLOW_ACK = 0x800000     # bit23
CS_UPDATE    = 0x4000000    # bit26
AUDIO_CLOCK_EN = 0x1        # AFMT_CNTL bit0
AUDIO_INFO_SEND = 0x10      # HDMI_INFOFRAME_CONTROL0 bit4


def replay_agnos_enable(bar, dig):
    """Run AGNOS's gpu_hdmi_audio_enable SEQUENCE (the process, incl. the disruptive edges a register
    snapshot cannot show) onto the LIVE, already-audibly-playing amdgpu path. Staged with an ear verdict
    after each edge so we localise WHICH step of agnos's process kills a working stream. This is the
    operator's 'write the driver on Linux to confirm the process works without agnos' — for the DCN half."""
    def R(n): return bar.rd(reg_addr(n, dig))
    def W(n, v): bar.wr(reg_addr(n, dig), v)

    print("=" * 74)
    print(" REPLAY AGNOS'S HDMI-AUDIO ENABLE SEQUENCE onto the LIVE amdgpu path")
    print(f" DIG {dig}.  START WITH AUDIO AUDIBLY PLAYING to the monitor and KEEP it playing.")
    print(" We apply agnos's process one edge at a time and you report, by ear, when it dies.")
    print(" !! One step drops the HDMI link for 120ms (agnos does this and recovers). If the DISPLAY")
    print("    does not come back, reboot — amdgpu is not told this happened. Nothing is written to disk.")
    print("=" * 74)
    orig = {n: R(n) for n in ("HDMI_GC", "DIG_BE_EN_CNTL", "AFMT_AUDIO_PACKET_CONTROL",
                              "AFMT_CNTL", "HDMI_INFOFRAME_CONTROL0")}
    print("  captured originals:", {k: hex(v) for k, v in orig.items()})

    def verdict(step):
        r = ask(f"    >>> {step}: is audio STILL playing?", ["y", "n", "q"])
        return r

    if ask("  begin?", ["y", "n"]) == "n":
        return

    # --- STEP 1: the non-disruptive re-writes agnos does (clock-en, info-send, drain re-arm).
    # These match amdgpu's end-state so they should be inert on a live stream — a control.
    W("AFMT_CNTL", R("AFMT_CNTL") | AUDIO_CLOCK_EN)
    W("HDMI_INFOFRAME_CONTROL0", R("HDMI_INFOFRAME_CONTROL0") | AUDIO_INFO_SEND)
    W("AFMT_AUDIO_PACKET_CONTROL", R("AFMT_AUDIO_PACKET_CONTROL") | CS_UPDATE | SAMPLE_SEND)
    time.sleep(0.3)
    if verdict("STEP 1 (clock/info/drain re-arm — expected inert)") == "q": return

    # --- STEP 2: agnos's Stage-1 LINK EVENT — SET AVMUTE, close tap, DIG_ENABLE DOWN 120ms, UP, re-arm.
    print("    STEP 2: applying agnos's DIG_ENABLE down->up link event (120ms; screen will blank)...")
    W("HDMI_GC", R("HDMI_GC") | AVMUTE)
    W("AFMT_AUDIO_PACKET_CONTROL", R("AFMT_AUDIO_PACKET_CONTROL") & ~SAMPLE_SEND)
    W("DIG_BE_EN_CNTL", R("DIG_BE_EN_CNTL") & ~DIG_ENABLE)         # link DOWN
    time.sleep(0.120)
    W("DIG_BE_EN_CNTL", R("DIG_BE_EN_CNTL") | DIG_ENABLE)          # link UP
    for _ in range(20000):
        if R("DIG_BE_EN_CNTL") & SYMCLK_BE_ON:
            break
    time.sleep(0.020)
    W("AFMT_AUDIO_PACKET_CONTROL", R("AFMT_AUDIO_PACKET_CONTROL") | OVERFLOW_ACK)
    W("HDMI_GC", R("HDMI_GC") & ~AVMUTE)                            # AVMUTE clear
    W("AFMT_AUDIO_PACKET_CONTROL", R("AFMT_AUDIO_PACKET_CONTROL") | CS_UPDATE | SAMPLE_SEND)
    time.sleep(0.3)
    v = verdict("STEP 2 (agnos's DIG link event — THE prime suspect)")
    if v == "q":
        return
    if v == "n":
        print("\n    ##### agnos's DIG_ENABLE link event KILLED a working stream — REPRODUCED on Linux. #####")
        print("    That edge is in gpu.cyr gpu_hdmi_audio_enable Stage 1. amdgpu never does it post-setup.")
        # try to recover the stream the way a fresh modeset would, to prove it's the edge not the end-state
        print("    Attempting recovery by re-asserting drain only (no link event)...")
        W("AFMT_AUDIO_PACKET_CONTROL", R("AFMT_AUDIO_PACKET_CONTROL") | OVERFLOW_ACK | CS_UPDATE | SAMPLE_SEND)
        time.sleep(0.3)
        verdict("recovery attempt (drain re-arm only)")

    # --- STEP 3: the post-feed AVMUTE pulse agnos does last (main.cyr gpu_hdmi_avmute_pulse).
    print("    STEP 3: agnos's terminal AVMUTE SET->CLEAR pulse (~48ms mute)...")
    W("HDMI_GC", R("HDMI_GC") | AVMUTE)
    time.sleep(0.048)
    W("HDMI_GC", R("HDMI_GC") & ~AVMUTE)
    time.sleep(0.3)
    verdict("STEP 3 (terminal AVMUTE pulse)")

    print("\n  restoring captured originals...")
    for n, v in orig.items():
        W(n, v)
    print("  done. If audio did not return, restart the tone / reboot. Reboot before normal use anyway.")
    print("=" * 74)


def main():
    ap = argparse.ArgumentParser(description="Kill-bisect / process-replay agnos DCN audio against live amdgpu (by ear).")
    ap.add_argument("--dev", default="0000:04:00.0")
    ap.add_argument("--dig", type=int, default=1, help="DIG instance (default 1 — the live encoder on archaemenid)")
    ap.add_argument("--read", metavar="WHAT", help="read-only: 'RAMP', 'ALL', or comma-separated names")
    ap.add_argument("--write", action="append", metavar="NAME=HEX", help="poke one register (repeatable)")
    ap.add_argument("--bisect", action="store_true", help="guided block-by-block kill-bisect")
    ap.add_argument("--replay-enable", action="store_true",
                    help="replay agnos's ENABLE SEQUENCE (edges incl. DIG link event) onto the live playing path")
    args = ap.parse_args()

    path = f"/sys/bus/pci/devices/{args.dev}/resource5"
    writable = bool(args.write or args.bisect or args.replay_enable)
    if writable and os.geteuid() != 0:
        sys.exit("writes need root")
    bar = Bar(path, writable)
    if args.replay_enable:
        replay_agnos_enable(bar, args.dig)
        return

    if args.read:
        if args.read.upper() == "RAMP":
            names = [f"AFMT_RAMP_CONTROL{i}" for i in range(4)]
        elif args.read.upper() == "ALL":
            names = list(REGS)
        else:
            names = [s.strip() for s in args.read.split(",")]
        do_read(bar, names, args.dig)
    if args.write:
        for spec in args.write:
            name, val = spec.split("=", 1)
            a = reg_addr(name, args.dig)
            old = bar.rd(a)
            bar.wr(a, int(val, 16))
            print(f"  {name}: 0x{old:08x} -> 0x{bar.rd(a):08x}")
    if args.bisect:
        bisect(bar, args.dig)
    if not (args.read or args.write or args.bisect):
        print("nothing to do — see --help")


if __name__ == "__main__":
    main()
