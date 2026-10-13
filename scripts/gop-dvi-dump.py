#!/usr/bin/env python3
# gop-dvi-dump.py — read-only DCN 2.1 register dump of the GOP-lit DVI pipe.
#
# A4 sub-arc, bite 1. Run under LINUX with amdgpu blacklisted
# (modprobe.blacklist=amdgpu on the kernel cmdline), so the console is the raw
# GOP/simple-framebuffer pipe — the same base pipe state agnos inherits. Diff the
# output against prior-art/dcn-audio-known-good-full-0716.txt (amdgpu HDMI playing)
# to see what a cold HDMI modeset changes that agnos's live DIG_MODE flip does not.
#
# ⚠ READ-ONLY BY CONSTRUCTION. BAR5 is opened O_RDONLY and mmap'd PROT_READ; there
# is no write path in this file at all. It cannot modeset, cannot blank the pipe.
# Worst case is a HANG on a first-touch read (group C) — recoverable with the
# power button, and every prior register is already fsync'd to disk by then.
#
# ⚠ RESOLUTION CAVEAT (read before interpreting): if the GOP handed Linux a
# different resolution than agnos gets from gnoboot (e.g. 800x600 vs 2560x1440),
# then the TIMING and PHY-LANE-RATE registers are confounded — a delta there is
# resolution, not DVI-vs-HDMI. Trust the CATEGORICAL reads: which DIG is live
# (DIG_MODE != 0), audio-block presence, DTO source select, and — the primary win
# — whether the 0x5Dxx PHY block reads without hanging. `fbset` / fb0 virtual_size
# tells you the current mode; it is stamped into the header below.
#
# NEVER add a write path here. If you need to write, that is a different, gated,
# recovery-scaffolded tool — not this one.

import mmap, os, struct, sys, time, subprocess

DEV = "0000:04:00.0"          # Cezanne iGPU (1002:1638)
RES5 = f"/sys/bus/pci/devices/{DEV}/resource5"

# Base-index dword bases (renoir_ip_offset.h). abs_dword already folds these in,
# so byte = abs_dword * 4 directly; base_idx is carried only for the reader.
# DIG_MODE encoding per the arc's iron-established mapping (0=DP-SST, 2=DVI, 3=HDMI).
# ⚠ 2 is DVI, NOT "reserved" — the whole A4 arc turns on "GOP lit DIG1 as DVI (mode 2),
# audio needs mode 3 (HDMI)". An earlier version of this table mislabeled 2.
DIG_MODE_NAMES = {0: "DP-SST", 1: "DP-alt", 2: "DVI", 3: "HDMI", 5: "DP-MST"}

# (name, base_idx, abs_dword). abs dwords verified against agnos gpu_regs.cyr
# (BASE_IDX2=0x34C0, BASE_IDX1=0xC0) and the design's dcn_2_1_0_offset.h decode.
GROUP_A = [  # known-safe: agnos already reads this class from its own code
    ("OTG0_OTG_CONTROL",        2, 0x34C0 + 0x1B41),
    ("OTG0_OTG_STATUS",         2, 0x34C0 + 0x1B49),
    ("OTG0_STATUS_FRAME_COUNT", 2, 0x34C0 + 0x1B4C),
    ("DIG0_DIG_FE_CNTL",        2, 0x34C0 + 0x2068),
    ("DIG1_DIG_FE_CNTL",        2, 0x34C0 + 0x2168),
    ("DIG0_DIG_BE_CNTL",        2, 0x34C0 + 0x20AF),
    ("DIG1_DIG_BE_CNTL",        2, 0x34C0 + 0x21AF),
    ("DIG0_DIG_BE_EN_CNTL",     2, 0x34C0 + 0x20B0),
    ("DIG1_DIG_BE_EN_CNTL",     2, 0x34C0 + 0x21B0),
    ("DIG0_DIG_LANE_ENABLE",    2, 0x34C0 + 0x20E1),
    ("DIG1_DIG_LANE_ENABLE",    2, 0x34C0 + 0x21E1),
    ("DIG1_HDMI_CONTROL",       2, 0x34C0 + 0x2071),
    ("DCCG_AUDIO_DTO_SOURCE",   1, 0x00C0 + 0x00AB),
    ("DCCG_AUDIO_DTO0_PHASE",   1, 0x00C0 + 0x00AC),
    ("DCCG_AUDIO_DTO0_MODULE",  1, 0x00C0 + 0x00AD),
]
GROUP_B = [  # new but high-value; the pll/clock-source question
    ("OTG0_PIXEL_RATE_CNTL",    1, 0x00C0 + 0x0080),  # [1:0] = pipe-0 clock source -> pll_id
    ("OTG0_DP_DTO0_PHASE",      1, 0x00C0 + 0x0081),  # live pixel clock (Hz) when MODULO==DPREFCLK
    ("OTG0_DP_DTO0_MODULO",     1, 0x00C0 + 0x0082),  # DPREFCLK (Hz)
    ("DCHUBBUB_ARB_DRAM_CNTL",  2, 0x39C8),           # did ATOM #13 act8 force self-refresh? bit1
]
GROUP_C = [  # ⚠ FIRST-TOUCH, HANG-CAPABLE. agnos has NEVER read this domain.
    # DIG0/PHY-A first (likely live), DIG1/PHY-B (RDPCSTX1) LAST — most likely gated.
    ("UNIPHYA_LINK_CNTL",       2, 0x5D2D),
    ("UNIPHYA_CHANNEL_XBAR",    2, 0x5D2E),
    ("RDPCSTX0_RDPCSTX_CNTL",   2, 0x5DF0),
    ("RDPCSTX0_PHY_CNTL0",      2, 0x5E00),
    ("RDPCSTX0_PHY_CNTL6",      2, 0x5E06),           # [.] MPLL_EN bits — the #12 no-op gate seed
    ("UNIPHYB_LINK_CNTL",       2, 0x5D2F),
    ("UNIPHYB_CHANNEL_XBAR",    2, 0x5D30),
    ("RDPCSTX1_RDPCSTX_CNTL",   2, 0x5EC8),           # RDPCSTX1 block — power-gated candidate, LAST
    ("RDPCSTX1_PHY_CNTL0",      2, 0x5ED8),
    ("RDPCSTX1_PHY_CNTL6",      2, 0x5EDE),
]

# Categorical decodes (resolution-independent — trust these even at a mismatched mode).
def decode_be(v):
    return dict(DIG_MODE=(v >> 16) & 7, DIG_FE_SOURCE_SELECT=(v >> 8) & 0x7F,
                DIG_DUAL_LINK=(v >> 0) & 1)
def decode_be_en(v):
    return dict(DIG_ENABLE=v & 1, DIG_SYMCLK_BE_ON=(v >> 8) & 1)
def decode_fe(v):
    return dict(DIG_SOURCE_SELECT=v & 7, DIG_SYMCLK_FE_ON=(v >> 24) & 1,
                TMDS_PIXEL_ENCODING=(v >> 28) & 1)


def main():
    if os.geteuid() != 0:
        sys.exit("error: must run as root (BAR5 mmap needs CAP_SYS_ADMIN):  sudo %s" % sys.argv[0])

    # Two captures, opposite driver requirements:
    #   default            — GOP-lit DVI pipe: amdgpu MUST be blacklisted.
    #   --under-amdgpu      — the HDMI-playing target: amdgpu MUST be bound + a tone playing.
    # Reading BAR5 read-only while amdgpu drives the display is safe and established
    # (the original dump-dcn-audio.py ran this way "while Firefox played").
    under_amdgpu = "--under-amdgpu" in sys.argv[1:]

    try:
        drv = os.path.basename(os.readlink(f"/sys/bus/pci/devices/{DEV}/driver"))
    except OSError:
        drv = "(none)"

    if under_amdgpu:
        if drv != "amdgpu":
            sys.exit("error: --under-amdgpu needs amdgpu BOUND (driver=%s). Boot normally (no blacklist) "
                     "and start a tone to the XB323U first." % drv)
    else:
        if drv == "amdgpu":
            sys.exit("error: amdgpu is bound to %s — this is the GOP-DVI capture, it needs the raw pipe.\n"
                     "  Either reboot with modprobe.blacklist=amdgpu, OR, if you meant the HDMI-target\n"
                     "  capture, re-run with:  sudo %s --under-amdgpu" % (DEV, sys.argv[0]))

    mode = "unknown"
    try:
        mode = open("/sys/class/graphics/fb0/virtual_size").read().strip()
    except OSError:
        pass

    ts = time.strftime("%Y-%m-%d %H:%M:%S")
    tag = "amdgpu-hdmi-phy" if under_amdgpu else "gop-dvi-readonly"
    outpath = f"/home/macro/Repos/agnosticos/docs/development/prior-art/dcn-{tag}-{time.strftime('%m%d')}.txt"
    out = open(outpath, "w")

    def line(s):
        print(s)
        out.write(s + "\n")
        out.flush()
        os.fsync(out.fileno())   # crash-survivable: a group-C hang keeps everything above

    what = "amdgpu-HDMI-playing pipe (target)" if under_amdgpu else "GOP-lit DVI pipe (agnos inherits)"
    line(f"# {what} — DCN read-only dump — {ts}")
    line(f"# device {DEV} driver={drv}  fb0 mode={mode}"
         + ("   ⚠ CONFIRM A TONE IS PLAYING to the XB323U for this to be a valid HDMI-playing capture"
            if under_amdgpu else ""))
    line(f"# ⚠ if mode != 2560x1440, timing + PHY-lane-rate reads are resolution-confounded;")
    line(f"#   trust the CATEGORICAL reads (DIG_MODE, audio-block, DTO source, PHY read-safety).")
    line(f"# diff target: prior-art/dcn-audio-known-good-full-0716.txt (amdgpu HDMI playing)")
    line("")

    fd = os.open(RES5, os.O_RDONLY | os.O_SYNC)
    size = os.fstat(fd).st_size
    mm = mmap.mmap(fd, size, mmap.MAP_SHARED, mmap.PROT_READ)

    def rd(abs_dword):
        b = abs_dword * 4
        if b + 4 > size:
            return None
        return struct.unpack_from("<I", mm, b)[0]

    def dump(group, label):
        line(f"=== {label} ===")
        for name, bi, absd in group:
            v = rd(absd)
            hv = "OOB" if v is None else f"0x{v:08x}"
            line(f"{name:<26} base{bi} abs=0x{absd:05x} {hv}")
            if v is not None and name.endswith("DIG_BE_CNTL"):
                d = decode_be(v)
                line(f"    DIG_MODE={d['DIG_MODE']} ({DIG_MODE_NAMES.get(d['DIG_MODE'],'?')})"
                     f"  FE_SOURCE_SELECT=0x{d['DIG_FE_SOURCE_SELECT']:x}")
            if v is not None and name.endswith("DIG_BE_EN_CNTL"):
                d = decode_be_en(v)
                line(f"    DIG_ENABLE={d['DIG_ENABLE']} SYMCLK_BE_ON={d['DIG_SYMCLK_BE_ON']}")
            if v is not None and name.endswith("DIG_FE_CNTL"):
                d = decode_fe(v)
                line(f"    SYMCLK_FE_ON={d['DIG_SYMCLK_FE_ON']} TMDS_PIXEL_ENCODING={d['TMDS_PIXEL_ENCODING']}")
        line("")

    dump(GROUP_A, "GROUP A — known-safe (agnos reads this class)")
    dump(GROUP_B, "GROUP B — new, high-value (pll/clock source)")

    # The phyid verdict re-reads only GROUP-A (safe) registers, so compute and emit
    # it BEFORE the hang-capable group C — a group-C lock must not cost us the
    # primary result. If C hangs, this verdict is already on disk.
    line("=== VERDICT (categorical, resolution-independent) ===")
    be0 = rd(0x34C0 + 0x20AF); be1 = rd(0x34C0 + 0x21AF)
    en0 = rd(0x34C0 + 0x20B0); en1 = rd(0x34C0 + 0x21B0)
    for idx, be, en in ((0, be0, en0), (1, be1, en1)):
        if be is None:
            continue
        m = (be >> 16) & 7
        live = "LIVE" if (en and (en & 1)) else "off"
        line(f"DIG{idx}: DIG_MODE={m} ({DIG_MODE_NAMES.get(m,'?')}) "
             f"FE_SOURCE_SELECT=0x{(be>>8)&0x7F:x} DIG_ENABLE={en&1 if en is not None else '?'}  -> {live}")
    line("# The DIG that is LIVE with DIG_MODE!=0 names the phyid the ATOM tables must target.")
    line("")

    # Group C LAST — the first agnos-side touch of 0x5Dxx, the hang candidate.
    line("=== GROUP C — ⚠ FIRST-TOUCH, may HANG. Everything above is already on disk. ===")
    dump(GROUP_C, "GROUP C — PHY / RDPCS (first agnos-side touch of 0x5Dxx)")
    line("# GROUP C completed without hanging -> the 0x5Dxx domain is read-safe for agnos.")

    mm.close(); os.close(fd); out.close()
    print(f"\nwrote {outpath}")


if __name__ == "__main__":
    main()
