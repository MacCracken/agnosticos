#!/usr/bin/env python3
"""M1·M2·M3 — decode + anchor-check the `/bin/modeset --dump` output from an iron boot.

The kernel dump prints `rd <off> <val>` (BASE_IDX 2) and `rd1 <off> <val>` (BASE_IDX 1), both DECIMAL, one
line per register. Decoding by eye is the exact trap that cost a burn (4294902881 misread as 0xFFFF0A61 when
it is 0xFFFF0461); this script maps each decimal offset to its NAME, prints the value in hex+decimal, and
checks the M1 anchors that must hold or the H7 transform is wrong and the ladder stops.

Usage:
    run /bin/klug > m1.txt           # on agnos, at the agnsh prompt
    # copy m1.txt out by mounting /dev/nvme0n1p2 ro from Linux, then:
    python3 scripts/m1-decode.py m1.txt

Exit 0 iff every anchor holds. The anchors (from gpu.md M1 card):
  H_TOTAL  0x1B2A = 2719   V_TOTAL 0x1B2F = 1480   V_TOTAL_MIN/_MAX 0x1B30/0x1B31 = 0 (GOP: DRR off)
  V_SYNC_A 0x1B37 = 0x00050000 (a 5-line pulse)    H_SYNC_A 0x1B2C = 0x00200000 (start 32)
  M3 scaler: SCL_MODE 0x0CEC != 6 (upscaling, not bypass) — SCL_MODE == 6 reopens P4's root cause.
  M2 phyid: needs the mdo_rd2a base-fix + re-burn (the 1.56.11 dump double-folded these); until then phyid=0
            rests on the VBIOS object-info derivation, not on M2.
"""
import re
import sys

# name table: (base, offset) -> name. base 2 = BASE_IDX 2 (rd), base 1 = BASE_IDX 1 (rd1).
NAMES = {
    (2, 0x1B2A): "OTG_H_TOTAL",           (2, 0x1B2B): "OTG_H_BLANK_START_END",
    (2, 0x1B2C): "OTG_H_SYNC_A",          (2, 0x1B2D): "OTG_H_SYNC_A_CNTL",
    (2, 0x1B2E): "OTG_H_TIMING_CNTL",     (2, 0x1B2F): "OTG_V_TOTAL",
    (2, 0x1B30): "OTG_V_TOTAL_MIN",       (2, 0x1B31): "OTG_V_TOTAL_MAX",
    (2, 0x1B33): "OTG_V_TOTAL_CONTROL",   (2, 0x1B36): "OTG_V_BLANK_START_END",
    (2, 0x1B37): "OTG_V_SYNC_A",          (2, 0x1B38): "OTG_V_SYNC_A_CNTL",
    (2, 0x1B39): "OTG_TRIGA_CNTL",        (2, 0x1B41): "OTG_CONTROL",
    (2, 0x1B44): "OTG_STATUS",            (2, 0x1B82): "OTG_STATIC_SCREEN_CONTROL",
    (2, 0x1B86): "OTG_CLOCK_CONTROL",     (2, 0x1B87): "OTG_VSTARTUP_PARAM",
    (2, 0x1B88): "OTG_VUPDATE_PARAM",     (2, 0x1B89): "OTG_VREADY_PARAM",
    (2, 0x1B8A): "OTG_MASTER_UPDATE_MODE",
    (2, 0x0528): "VTG0_CONTROL",
    (2, 0x1ACA): "OPTC_UNDERFLOW",        (2, 0x1ACB): "OPTC_DATA_SOURCE_SELECT",
    (2, 0x1ACC): "OPTC_DATA_FORMAT_CONTROL", (2, 0x1ACF): "OPTC_INPUT_CLOCK_CONTROL",
    (2, 0x1AD0): "OPTC_MEMORY_CONFIG",
    (2, 0x183F): "FMT_DYNAMIC_EXP_CNTL",  (2, 0x1840): "FMT_CONTROL",
    (2, 0x1841): "FMT_BIT_DEPTH_CONTROL", (2, 0x1845): "FMT_CLAMP_CNTL",
    (2, 0x1849): "FMT_422_CONTROL",       (2, 0x1854): "DPG_CONTROL",
    (2, 0x1884): "OPPBUF_CONTROL",        (2, 0x188C): "OPP_PIPE_CONTROL",
    (2, 0x1271): "MPCC0_TOP_SEL",         (2, 0x1272): "MPCC0_BOT_SEL",
    (2, 0x1273): "MPCC0_MPCC_CONTROL",    (2, 0x1274): "MPCC0_MPCC_STATUS",
    (2, 0x127F): "MPCC0_MPCC_MEM_PWR",    (2, 0x1385): "MPC_OUT0_MUX",
    (2, 0x05E5): "HUBP_SURFACE_PIXEL_FMT",(2, 0x05E7): "HUBP_SURFACE_CONFIG",
    (2, 0x05EA): "HUBP_VIEWPORT",         (2, 0x05F3): "DCHUBP_CNTL",
    (2, 0x05F4): "DCHUBP_CNTL2",          (2, 0x0607): "HUBP_SURFACE_PITCH",
    (2, 0x060A): "HUBP_SURF_ADDR",        (2, 0x060B): "HUBP_SURF_ADDR_HI",
    (2, 0x061A): "HUBP_DCSURF_FLIP_CTRL", (2, 0x066C): "HUBPRET0_CONTROL",
    (2, 0x0080): "DCPG_DOMAIN0_PG_CONFIG",(2, 0x0082): "DCPG_DOMAIN1_PG_CONFIG",
    (2, 0x00B2): "DC_IP_REQUEST_CNTL",
    (1, 0x40): "OTG_PIXEL_RATE_CNTL(b1)", (1, 0x41): "OTG_PIXEL_RATE_CNTL1(b1)",
    (1, 0x80): "DCCG_DEEP_COLOR_CNTL(b1)",(1, 0x81): "DCCG_DISP_CNTL(b1)",
    (1, 0x82): "DCCG_DISP_CNTL1(b1)",     (1, 0x99): "SYMCLK(b1)",
    (1, 0x9A): "SYMCLK1(b1)",             (1, 0x9B): "SYMCLK2(b1)",
    (1, 0x9C): "SYMCLK3(b1)",             (1, 0xB6): "DCCG_AUDIO_DTO(b1)",
    (1, 0xAB): "DCCG_AUDIO_DTO_SOURCE(b1)",(1, 0xAC): "DCCG_AUDIO_DTO0_PHASE(b1)",
    (1, 0xAD): "DCCG_AUDIO_DTO0_MODULE(b1)",
    # M2 PHY/RDPCS — ABSOLUTE dwords (the kernel reads them via mdo_rd2a, base subtracted). Names + expected
    # values from the 07-20 GOP-DVI capture (dcn-gop-dvi-readonly-0720.txt).
    (2, 0x5D2D): "UNIPHYA_LINK_CNTL",     (2, 0x5D2E): "UNIPHYA_CHANNEL_XBAR",
    (2, 0x5D2F): "UNIPHYB_LINK_CNTL",     (2, 0x5D30): "UNIPHYB_CHANNEL_XBAR",
    (2, 0x5DF0): "RDPCSTX0_RDPCSTX_CNTL", (2, 0x5E00): "RDPCSTX0_PHY_CNTL0",
    (2, 0x5E06): "RDPCSTX0_PHY_CNTL6",    (2, 0x5EC8): "RDPCSTX1_RDPCSTX_CNTL",
    (2, 0x5ED8): "RDPCSTX1_PHY_CNTL0",    (2, 0x5EDE): "RDPCSTX1_PHY_CNTL6",
    (2, 0x0CEC): "DSCL0_SCL_MODE",        (2, 0x0CF1): "SCL_HORZ_FILTER_SCALE_RATIO",
    (2, 0x0D03): "DSCL0_RECOUT_SIZE",     (2, 0x016B): "DPP_STRIDE",
    # M8b — the EXACT read-set ATOM #76 (DIG1TransmitterControl ENABLE) touches, from the
    # atom-interp.py oracle (21 reads / 17 writes / 5 delays over 16 distinct indices). These seed
    # the H5 snapshot-DRY so the interpreter takes the branches iron would take. ABSOLUTE dwords.
    (2, 0x5535): "UNIPHY_A_LINK?(76rd)",  (2, 0x556F): "UNIPHY_PHY_POWER(76rw)",
    (2, 0x5570): "UNIPHY_PHY_POWER1(76rw)", (2, 0x55A1): "UNIPHY_CTRL(76rw)",
    (2, 0x5DE9): "RDPCSTX0_?(76rw)",      (2, 0x5DF7): "RDPCSTX0_?(76rd)",
    (2, 0x5DFC): "RDPCSTX0_?(76rd)",      (2, 0x5E03): "RDPCSTX0_LANE_RESET(76rw)",
    (2, 0x5E0F): "RDPCSTX0_?(76rw)",      (2, 0x5E10): "RDPCSTX0_?(76rw)",
    (2, 0x5E11): "RDPCSTX0_?(76rw)",      (2, 0x5E12): "RDPCSTX0_?(76rw)",
    # The per-instance DIG BACK END — what ATOM's `phyid` indexes. DIG_BE_CNTL abs = 0x556F + N*0x100,
    # DIG_BE_EN_CNTL one dword higher. Instance 0 is in the 76readset group above.
    (2, 0x566F): "DIG_BE_CNTL[1]",        (2, 0x5670): "DIG_BE_EN_CNTL[1]",
    (2, 0x576F): "DIG_BE_CNTL[2]",        (2, 0x5770): "DIG_BE_EN_CNTL[2]",
    (2, 0x586F): "DIG_BE_CNTL[3]",        (2, 0x5870): "DIG_BE_EN_CNTL[3]",
    (2, 0x596F): "DIG_BE_CNTL[4]",        (2, 0x5970): "DIG_BE_EN_CNTL[4]",
}

# The 16 distinct indices ATOM #76 READS. A snapshot-seeded DRY (H5) is only meaningful once every one of
# these has a real captured value — with no seed they read 0 and the interpreter may branch differently
# than it will on iron. m1-decode reports coverage so the M8b gate is mechanical, not a judgement call.
#
# ⚠⚠ THE SEEDED DRY IS DIAGNOSTIC — AND ON 2026-07-24 IT OVERTURNED MD-2. Four runs of ATOM #76:
#     no seed,        phyid=0 -> reads=21      writes=17  delays=5
#     placeholders,   phyid=0 -> reads=87804   writes=20  delays=43895
#     REAL iron seed, phyid=0 -> reads=87292   writes=20  delays=43639   <- POLL STORM
#     REAL iron seed, phyid=1 -> reads=21      writes=17  delays=5       <- CLEAN
# The storm is NOT an artifact of static snapshots (that was the first, wrong reading — phyid=1 falsifies
# it: a frozen snapshot satisfies the polls perfectly well when the values are the LIVE ones). It is the
# signal that phyid=0 targets an INACTIVE PHY whose status bits never read ready. With real values the
# correct phyid completes and the wrong one hangs, which is exactly what makes this test worth running.
# So: read the WRITE LIST, and treat a storm as "wrong target", not "bad decoder".
# (agnos's own interpreter is bounded by a 1,000,000-step cap, so neither case can wedge the kernel.)
ATOM76_READS = [0x5535, 0x556F, 0x5570, 0x55A1, 0x5D2D, 0x5D2E, 0x5DE9, 0x5DF0,
                0x5DF7, 0x5DFC, 0x5E03, 0x5E06, 0x5E0F, 0x5E10, 0x5E11, 0x5E12]

# ── COORDINATE SYSTEMS: three tags, one absolute space ────────────────────────────────────────────
# ⛔⛔ THE DEFECT THIS CLOSES (found 2026-07-30 while seeding #12): the kernel's `mdo_rd2` prints
# BASE_IDX-2-RELATIVE offsets and `mdo_rd2a` prints ABSOLUTE ones, and until 1.56.33 BOTH tagged their
# line `rd ` — in a scheme where `rd1` exists for no other purpose than telling this decoder which base
# a line is in. This script wrote every `rd` line into the ATOM seed AS ABSOLUTE, so 87 of the last
# capture's 97 entries were relative offsets labelled absolute (H_TOTAL's value filed at 0x1B2A when
# its absolute index is 0x4FEA). Harmless only by luck: #4/#76/#12 read solely the mdo_rd2a groups.
#
# ⭐ THE TRANSFORM IS PROVEN BY THE CAPTURE ITSELF, not asserted. `rd 6977` (relative 0x1B41) and
# `rd 20481` (absolute 0x5001) are the SAME REGISTER read by the two different printers in one dump,
# and 0x1B41 + 0x34C0 == 0x5001. They must carry the same value; XCHECKS below makes that mechanical.
BASE_DCN_1 = 0xC0        # kernel GPU_BASE_DCN_1
BASE_DCN_2 = 0x34C0      # kernel GPU_BASE_DCN_2

# Groups whose `rd` lines are ABSOLUTE in LEGACY captures (pre-1.56.33, before the `rda` tag existed).
# From the kernel's own mdo_dump call sites — these four use mdo_rd2a, everything else uses mdo_rd2.
LEGACY_ABS_GROUPS = ("M2-PHY/RDPCS", "M8-76readset", "M8-digbe", "M12-pixclk")

# Registers reachable through BOTH printers in one dump. Each is (relative_tag, relative_off, abs_off).
# ⚠ A pair that is ABSENT from a capture must report "not present", never pass silently — a check that
# cannot fail has not passed, and this file's whole subject is a conflation that looked fine for weeks.
XCHECKS = [("rd",  0x1B41, 0x5001, "OTG_CONTROL"),
           ("rd1", 0x0082, 0x0142, "DP_DTO_MODULO")]

LINE = re.compile(r"^\s*(rd1|rda|rd)\s+(\d+)\s+(\d+)\s*$")
GROUP = re.compile(r"^\s*modeset: dump group\s+(\S+)")


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: m1-decode.py <m1.txt>")
    vals = {}      # (base, off) -> value          — as PRINTED, for naming + the existing anchors
    absv = {}      # absolute dword -> value       — the ATOM coordinate system, for the seed
    raw = []       # (tag, off, val, group)
    group = "?"
    with open(sys.argv[1], errors="replace") as f:
        for ln in f:
            g = GROUP.match(ln)
            if g:
                group = g.group(1)
                continue
            m = LINE.match(ln)
            if not m:
                continue
            raw.append((m.group(1), int(m.group(2)), int(m.group(3)), group))

    groups_seen = {g for _, _, _, g in raw}
    have_rda = any(t == "rda" for t, _, _, _ in raw)
    legacy = 0
    for tag, off, val, grp in raw:
        if tag == "rd1":
            base, abs_off = 1, off + BASE_DCN_1
        elif tag == "rda":
            base, abs_off = 2, off
        else:
            # ⚠ LEGACY BRANCH. Before 1.56.33 mdo_rd2a shared this tag, so `rd` alone does not say
            # which coordinate system the number is in. Resolve by the group header — the only other
            # evidence the capture carries — and COUNT it, so the report says how much of this decode
            # rests on a heuristic rather than on a self-describing line.
            if not have_rda and grp in LEGACY_ABS_GROUPS:
                base, abs_off = 2, off
                legacy += 1
            else:
                base, abs_off = 2, off + BASE_DCN_2
        vals[(base, off)] = val
        absv[abs_off] = val

    print(f"parsed {len(raw)} register reads")
    if have_rda:
        print("  tags: rd / rd1 / rda present -- every line is SELF-DESCRIBING (kernel >= 1.56.33)\n")
    else:
        print(f"  ⚠ LEGACY capture (no `rda` tag): {legacy} `rd` lines resolved as ABSOLUTE by group")
        print(f"    header {LEGACY_ABS_GROUPS}, the rest as BASE_IDX-2 relative (+0x{BASE_DCN_2:X}).")
        print("    Re-dump on a >= 1.56.33 kernel to make this mechanical instead of inferred.\n")

    # ── the transform's own gate: the same register through two different printers ────────────────
    print("=== COORDINATE CROSS-CHECK (the capture proves its own transform, or says it cannot) ===")
    xfail = 0
    for tag, rel, ab, name in XCHECKS:
        rb = 1 if tag == "rd1" else 2
        # ⛔ THE FIRST DRAFT OF THIS CHECK COULD NOT FAIL, and two mutations proved it. It compared the
        # two raw PRINTED values — which are equal because both printers read the same silicon, no
        # matter what arithmetic this script does — so corrupting BASE_DCN_2 left it green. The claim
        # under test is about the INDEX MAPPING, so the mapping is what must be asserted:
        #   (1) rel + base == abs        the base constant itself
        #   (2) the two reads agree      the two printers really did hit one register
        #   (3) absv[abs] == that value  normalisation actually FILED it there
        # (3) is what catches a mis-resolved group; (1) is what catches a wrong base.
        a = vals.get((rb, rel))
        b = vals.get((2, ab))
        if a is None or b is None:
            miss = "relative" if a is None else "absolute"
            print(f"  n/a   {name}: the {miss} read is NOT in this capture -- proves nothing here")
            continue
        base = BASE_DCN_1 if tag == "rd1" else BASE_DCN_2
        mapped = rel + base
        if mapped != ab:
            print(f"  FAIL  {name}: rel 0x{rel:04X} + 0x{base:X} = 0x{mapped:04X}, but the absolute "
                  f"read of the same register is 0x{ab:04X}  ⛔ THE BASE CONSTANT IS WRONG")
            xfail += 1
        elif a != b:
            print(f"  FAIL  {name}: rel 0x{rel:04X}=0x{a:08X} != abs 0x{ab:04X}=0x{b:08X}"
                  f"  ⛔ two printers, one register, two values")
            xfail += 1
        elif absv.get(ab) != a:
            got = absv.get(ab)
            got_s = "absent" if got is None else f"0x{got & 0xFFFFFFFF:08X}"
            print(f"  FAIL  {name}: 0x{ab:04X} normalised to {got_s}, want 0x{a & 0xFFFFFFFF:08X}"
                  f"  ⛔ a group was resolved into the WRONG coordinate system")
            xfail += 1
        else:
            print(f"  OK    {name}: rel 0x{rel:04X} + 0x{base:X} = abs 0x{ab:04X}, both read "
                  f"0x{a & 0xFFFFFFFF:08X}, filed there")
    print()

    print(f"{'reg':<32} {'off':>7}  {'hex':>10}  {'dec':>12}")
    for (base, off) in sorted(vals, key=lambda k: (k[0], k[1])):
        v = vals[(base, off)]
        nm = NAMES.get((base, off), f"?base{base}?")
        tag = f"0x{off:04X}" if base == 2 else f"b1:0x{off:02X}"
        print(f"{nm:<32} {tag:>7}  0x{v & 0xFFFFFFFF:08X}  {v:>12}")

    print("\n=== ANCHORS (M1 card — all must hold, or the H7 transform is wrong) ===")
    fails = 0

    def chk(cond, label, got):
        nonlocal fails
        if cond:
            print(f"  OK    {label}  (got {got})")
        else:
            print(f"  FAIL  {label}  (got {got})")
            fails += 1

    def g(base, off):
        return vals.get((base, off))

    chk(g(2, 0x1B2A) == 2719, "H_TOTAL 0x1B2A == 2719", g(2, 0x1B2A))
    chk(g(2, 0x1B2F) == 1480, "V_TOTAL 0x1B2F == 1480", g(2, 0x1B2F))
    chk(g(2, 0x1B37) == 0x00050000, "V_SYNC_A 0x1B37 == 0x00050000 (5-line pulse)", g(2, 0x1B37))
    chk(g(2, 0x1B2C) == 0x00200000, "H_SYNC_A 0x1B2C == 0x00200000 (start 32)", g(2, 0x1B2C))
    # DRR clamps: GOP does a fixed-mode set and leaves these 0 (amdgpu programs 1480). V_TOTAL_CONTROL=0
    # means the OTG ignores MIN/MAX and rasters off V_TOTAL. A GOP-vs-amdgpu delta, NOT a transform gate.
    chk(g(2, 0x1B30) == 0, "V_TOTAL_MIN 0x1B30 == 0 (GOP: DRR off)", g(2, 0x1B30))
    chk(g(2, 0x1B31) == 0, "V_TOTAL_MAX 0x1B31 == 0 (GOP: DRR off)", g(2, 0x1B31))
    scl = g(2, 0x0CEC)
    chk(scl is not None and (scl & 0x7) != 6, "M3 SCL_MODE[2:0] 0x0CEC != 6 (upscaling, not bypass)", scl)
    # ⚠ M2 phyid cross-check requires the mdo_rd2a base-fix (fixed 2026-07-24; the 1.56.11 dump double-folded
    # the M2 offsets and read garbage). On a re-burn, the check is: UNIPHYA_LINK_CNTL 0x5D2D reads the 07-20
    # value 0x01000100 (not 0/0xFFFFFFFF). Until re-burned, phyid=0 rests on the VBIOS object-info derivation.
    uni = g(2, 0x5D2D)
    if uni == 0x01000100:
        print(f"  OK    M2 UNIPHYA_LINK_CNTL 0x5D2D == 0x01000100 (base-fix landed, PHY read valid)  (got {uni})")
    else:
        print(f"  NOTE  M2 0x5D2D = {uni} (not the 07-20 0x01000100) — needs the mdo_rd2a base-fix + re-burn; not a gate")

    # === M8b: ATOM #76 read-set coverage + the H5 snapshot seed ===
    # The snapshot-seeded DRY is only meaningful when every index #76 reads has a REAL captured value.
    # Report coverage mechanically, and emit the seed file atom-interp.py --regsnapshot consumes.
    print("\n=== M8b: ATOM #76 read-set coverage (H5 snapshot seed) ===")
    # ⛔ MEASURED IN `absv`, NOT `vals` — 1.56.33. This looked up the offset AS PRINTED, which made it
    # blind to the very thing it exists to certify: coverage is a claim about the SEED's index space,
    # and the seed is written from absv. Emptying LEGACY_ABS_GROUPS (i.e. mis-converting every absolute
    # group) left this reporting a serene 16/16 while the seed's indices were all wrong. A gate that
    # reads a different variable from the artifact it certifies is not a gate.
    have = [o for o in ATOM76_READS if o in absv]
    miss = [o for o in ATOM76_READS if o not in absv]
    print(f"  captured {len(have)}/{len(ATOM76_READS)} of the indices #76 reads")
    if miss:
        print("  MISSING: " + " ".join(f"0x{o:04X}" for o in miss))
        print("  => the H5 snapshot-DRY of #76 is NOT yet meaningful; those reads would return 0.")
    else:
        print("  => full coverage: the snapshot-DRY of #76 can take iron's branches.")

    # ⭐ AND IF THE CAPTURE CARRIES THE GROUP, THE INDICES MUST SURVIVE NORMALISATION. Partial coverage
    # is legitimately informational — an old capture may simply not have read those registers. But a
    # capture that DID read them, whose indices then fail to appear in the absolute space, is a
    # CONVERSION bug, not a missing capture, and the two must not report the same way.
    # ⛔ This is the assertion that catches a mis-resolved group: emptying LEGACY_ABS_GROUPS takes
    # coverage 16/16 -> 0/16 while every other check in this file stays green.
    if "M8-76readset" in groups_seen and miss:
        print(f"  ⛔ FAIL: the M8-76readset group IS in this capture, yet {len(miss)} of its indices")
        print("     are absent from the absolute space. That is a normalisation defect, not a gap.")
        xfail += 1

    # === MD-2: which link encoder does ATOM's `phyid` have to name? ===
    # Mirrors kernel gpu_phy_discover() and atom-interp.py's derive_phyid_from_snapshot(): the live
    # instance is ENABLED, has a front end routed, and carries a real signalling mode. Reported here so a
    # capture ANSWERS the question instead of leaving it to a VBIOS enum guess — the guess said 0, the
    # answer is 1, and #76 at 0 would have power-cycled a dead PHY.
    print("\n=== MD-2: phyid, derived from the live DIG back end ===")
    phy = None
    for i in range(5):
        be = vals.get((2, 0x556F + i * 0x100))
        en = vals.get((2, 0x5570 + i * 0x100))
        if be is None or en is None:
            print(f"  inst {i}: not captured")
            continue
        mode = (be >> 16) & 0x7
        fesel = (be >> 8) & 0xFF
        live = bool(en & 1) and fesel != 0 and mode in (2, 3)
        tag = "  <== LIVE" if live else ""
        print(f"  inst {i}: BE=0x{be:08X} mode={mode} FE_SOURCE=0x{fesel:02X} "
              f"EN=0x{en:08X} enable={en & 1}{tag}")
        if live and phy is None:
            phy = i
    if phy is None:
        print("  => phyid UNDETERMINED from this capture (missing DIG_BE regs, or nothing live).")
        print("     ⛔ Do NOT run ATOM #76 on a guess — it power-cycles whichever PHY it is told to.")
    else:
        print(f"  => phyid = {phy}")

    if len(sys.argv) > 2:
        seed_path = sys.argv[2]
        with open(seed_path, "w") as sf:
            sf.write("# H5 snapshot seed for atom-interp.py --regsnapshot\n")
            sf.write("# Captured from an agnos iron dump (`run /bin/modeset --dump`), decoded by m1-decode.py.\n")
            sf.write("# Format: IDX VALUE (both hex). IDX is the ABSOLUTE dword the ATOM tables use.\n")
            # ⛔ THIS LOOP USED TO WRITE THE OFFSET AS PRINTED and call it absolute, which was true for
            # the mdo_rd2a groups and FALSE for every other `rd` line (+0x34C0) and every `rd1` line
            # (+0xC0). #4/#76/#12 happened to read only the absolute-printed groups, so no seed that
            # ever mattered was wrong -- but the cold path needs raster registers, which are exactly
            # the ones that were mislabelled. Now every entry goes through the same normalisation, and
            # the CROSS-CHECK above proves that normalisation against the capture itself.
            # ⛔⛔ 0xFFFFFFFF IS NOT A REGISTER VALUE, IT IS THE APERTURE SAYING "NOTHING HERE" — and
            # on 2026-07-30 a wide PLL-block read POISONED the aperture mid-dump: from 0x5FA5 onward
            # every read in that boot returned all-ones, including registers that had returned real
            # values on the previous burn. Seeding those would hand the interpreter confident garbage
            # and steer it down branches iron never takes, which is strictly worse than an absent
            # entry: an absent one reads 0 and shows up in the unseeded count, a poisoned one lies.
            poison = [a for a in absv if absv[a] & 0xFFFFFFFF == 0xFFFFFFFF]
            for ab in sorted(absv):
                if absv[ab] & 0xFFFFFFFF == 0xFFFFFFFF:
                    continue
                sf.write(f"0x{ab:04X} 0x{absv[ab] & 0xFFFFFFFF:08X}\n")
            if poison:
                print(f"  ⛔ DROPPED {len(poison)} all-ones (0xFFFFFFFF) reads from the seed —"
                      f" first at 0x{min(poison):04X}.")
                print("     An all-ones read is a dead/poisoned aperture, not a value. If they run")
                print("     CONTIGUOUSLY to the end of a group, suspect a read that killed the")
                print("     aperture and treat every later register in that boot as VOID.")
        print(f"  wrote snapshot seed -> {seed_path}")

    print()
    # ⛔ THE CROSS-CHECK MUST BE ABLE TO FAIL THE RUN, not merely print. It was written reporting-only
    # in the first draft, which would have made it a citation rather than a gate — the exact shape this
    # file's own subject already cost the tree once.
    if xfail:
        print(f"=== M1 decode: {xfail} COORDINATE CHECK(S) FAILED — a base constant or a group")
        print("    resolution is wrong, so the seed's indices are in the wrong space.")
        print("    Do NOT feed it to atom-interp.py: a wrong-index seed reads as zeros, and a")
        print("    zero-seeded dry run is exactly what #76 taught us not to trust. ===")
        return 1
    if fails == 0:
        print("=== M1 decode: ALL ANCHORS HOLD — the H7 transform is iron-attested (M2 phyid rides a re-burn) ===")
        return 0
    print(f"=== M1 decode: {fails} ANCHOR(S) FAILED — do not proceed to the write bites ===")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
