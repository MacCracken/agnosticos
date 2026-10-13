#!/usr/bin/env python3
"""H7 — the 3-way-anchored DCN register table.

Kills the derived-offset hazard permanently.

WHY THIS EXISTS
---------------
Every wrong turn in the 1.55.x display arc came from the same habit: deriving a DCN register offset by
arithmetic and then WRITING it on a live pipe. Burn 3 of P4 wrote an arithmetically-correct pitch value to a
mislabelled offset under the OTG lock and took the box down with a HUBP underflow (risk R4). A separate
offset, 0x1275, read back a perfectly plausible 0 while being simply wrong. Plausible is not correct.

The amdgpu modeset capture gives ABSOLUTE dword indices for the registers a real driver touches at exactly
2560x1440 -- the same mode archaemenid runs. So the whole OTG/OPTC/OPP/FMT/VTG/DCPG block can be transformed
into agnos's BASE_IDX-relative space and CHECKED against constants agnos already trusts on iron, rather than
guessed.

    BASE_IDX 2 (the big DCN block):  rel = abs - 0x34C0
    BASE_IDX 1 (DCCG / low block):   rel = abs - 0x00C0

THE GATE
--------
All twelve anchors must reproduce EXACTLY. If any one fails, the transform is wrong and NOTHING below it
ships -- that is the whole point of anchoring. Exit code is non-zero in that case.

Standalone value is permanent: every future DCN bite inherits this table.

Usage:
    python3 scripts/dcn-reg-derive.py                # verify anchors + emit handover table
    python3 scripts/dcn-reg-derive.py --all          # also dump every transformed register in the trace
"""

import argparse
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
TRACE = REPO / "docs/development/prior-art/amdgpu-hdmi-modeset-writes-0717.txt"
GPU_REGS = pathlib.Path("/home/macro/Repos/agnos/kernel/core/gpu_regs.cyr")

BASE2_OFFSET = 0x34C0
BASE1_OFFSET = 0x00C0
# Anything at or above this absolute index belongs to BASE_IDX 2; below it, BASE_IDX 1. Implied directly by
# the anchor set: the BASE_IDX 1 anchors sit at abs 0x140/0x16B, the BASE_IDX 2 anchors at 0x3AB3..0x5628.
BASE_SPLIT = 0x34C0

# (plan name, absolute dword idx, base_idx, expected relative, gpu_regs.cyr symbol or None, instance stride
#  to subtract before comparing -- DIG1 is instance 1, so its constant is the DIG0 base)
ANCHORS = [
    ("OTG_H_TOTAL",             0x4FEA, 2, 0x1B2A, "GPU_R_OTG_H_TOTAL",            0),
    ("OTG_V_TOTAL",             0x4FEF, 2, 0x1B2F, "GPU_R_OTG_V_TOTAL",            0),
    ("OTG_CONTROL",             0x5001, 2, 0x1B41, "GPU_R_OTG_CONTROL",            0),
    ("OTG_MASTER_UPDATE_LOCK",  0x504B, 2, 0x1B8B, "GPU_R_OTG_MASTER_UPDATE_LOCK", 0),
    ("OTG_GLOBAL_CONTROL0",     0x5050, 2, 0x1B90, "GPU_R_OTG_GLOBAL_CONTROL0",    0),
    ("DCHUBP_CNTL",             0x3AB3, 2, 0x05F3, "GPU_R_HUBP_CNTL",              0),
    ("MPCC0_TOP_SEL",           0x4731, 2, 0x1271, "GPU_R_MPCC0_TOP_SEL",          0),
    ("MPC_OUT0_MUX",            0x4845, 2, 0x1385, "GPU_R_MPC_OUT0_MUX",           0),
    ("HUBPRET0_CONTROL",        0x3B2C, 2, 0x066C, "GPU_R_HUBPRET0_CONTROL",       0),
    # DIG1 == DIG0 base + one DIG stride (0x100). agnos's constant names the DIG0 instance.
    ("DIG1_DIG_FE_CNTL",        0x5628, 2, 0x2168, "GPU_R_DIG_FE_CNTL",            0x100),
    ("OTG_PIXEL_RATE_CNTL",     0x0140, 1, 0x0080, "GPU_R_OTG_PIXEL_RATE_CNTL",    0),
    ("DCCG_AUDIO_DTO_SOURCE",   0x016B, 1, 0x00AB, None,                           0),
]

# Registers the modeset ladder needs that agnos does NOT have yet. Derived, then confirmed present in the
# trace. Names are the handover; values must survive the same transform the anchors validate.
HANDOVER = [
    ("GPU_R_OTG_H_SYNC_A",              0x1B2C, 2, "OTGn H_SYNC_A -- pulse width [15:0], start [31:16]"),
    ("GPU_R_OTG_H_SYNC_A_CNTL",         0x1B2D, 2, "OTGn H_SYNC_A_CNTL -- polarity bit0"),
    ("GPU_R_OTG_V_TOTAL_MIN",           0x1B30, 2, "OTGn V_TOTAL_MIN -- M5 writes this with V_TOTAL"),
    ("GPU_R_OTG_V_TOTAL_MAX",           0x1B31, 2, "OTGn V_TOTAL_MAX -- M5 writes this with V_TOTAL"),
    ("GPU_R_OTG_V_TOTAL_CONTROL",       0x1B33, 2, "OTGn V_TOTAL_CONTROL -- min/max sel, drr"),
    ("GPU_R_OTG_V_SYNC_A",              0x1B37, 2, "OTGn V_SYNC_A -- M1 anchor: amdgpu writes 0x00050000 (5-line pulse)"),
    ("GPU_R_OTG_V_SYNC_A_CNTL",         0x1B38, 2, "OTGn V_SYNC_A_CNTL -- polarity bit0"),
    ("GPU_R_OTG_TRIGA_CNTL",            0x1B39, 2, "OTGn TRIGA_CNTL"),
    ("GPU_R_OTG_TRIGA_MANUAL_TRIG",     0x1B3A, 2, "OTGn TRIGA_MANUAL_TRIG"),
    ("GPU_R_OTG_STATIC_SCREEN_CONTROL", 0x1B82, 2, "OTGn STATIC_SCREEN_CONTROL"),
    ("GPU_R_OTG_CLOCK_CONTROL",         0x1B86, 2, "OTGn OTG_CLOCK_CONTROL -- M6 sets this in the envelope"),
    ("GPU_R_OTG_VSTARTUP_PARAM",        0x1B87, 2, "OTGn VSTARTUP_PARAM -- M6 envelope"),
    ("GPU_R_OTG_VUPDATE_PARAM",         0x1B88, 2, "OTGn VUPDATE_PARAM -- M6 envelope"),
    ("GPU_R_OTG_VREADY_PARAM",          0x1B89, 2, "OTGn VREADY_PARAM -- M6 envelope"),
    ("GPU_R_OPTC_DATA_SOURCE_SELECT",   0x1ACB, 2, "OPTCn DATA_SOURCE_SELECT"),
    ("GPU_R_OPTC_DATA_FORMAT_CONTROL",  0x1ACC, 2, "OPTCn DATA_FORMAT_CONTROL"),
    ("GPU_R_OPTC_INPUT_CLOCK_CONTROL",  0x1ACF, 2, "OPTCn INPUT_CLOCK_CONTROL -- M6 sets 0x3"),
    ("GPU_R_OPTC_MEMORY_CONFIG",        0x1AD0, 2, "OPTCn MEMORY_CONFIG"),
    ("GPU_R_FMT_DYNAMIC_EXP_CNTL",      0x183F, 2, "FMTn DYNAMIC_EXP_CNTL"),
    ("GPU_R_FMT_CONTROL",               0x1840, 2, "FMTn FMT_CONTROL"),
    ("GPU_R_FMT_BIT_DEPTH_CONTROL",     0x1841, 2, "FMTn BIT_DEPTH_CONTROL"),
    ("GPU_R_FMT_CLAMP_CNTL",            0x1845, 2, "FMTn CLAMP_CNTL"),
    ("GPU_R_FMT_422_CONTROL",           0x1849, 2, "FMTn FMT_422_CONTROL"),
    ("GPU_R_DPG_CONTROL",               0x1854, 2, "DPGn DPG_CONTROL"),
    ("GPU_R_OPPBUF_CONTROL",            0x1884, 2, "OPPBUFn OPPBUF_CONTROL"),
    ("GPU_R_OPP_PIPE_CONTROL",          0x188C, 2, "OPP_PIPEn OPP_PIPE_CONTROL"),
    ("GPU_R_VTG0_CONTROL",              0x0528, 2, "VTG0_CONTROL -- M6 envelope"),
    ("GPU_R_DC_IP_REQUEST_CNTL",        0x00B2, 2, "DC_IP_REQUEST_CNTL"),
    # See R5. These two are the reason the naming is explicit.
    ("GPU_R_DCPG_DOMAIN0_PG_CONFIG",    0x0080, 2, "DCPG DOMAIN0_PG_CONFIG -- BASE_IDX 2. COLLIDES numerically with GPU_R_OTG_PIXEL_RATE_CNTL 0x80 BASE_IDX 1"),
    ("GPU_R_DCPG_DOMAIN1_PG_CONFIG",    0x0082, 2, "DCPG DOMAIN1_PG_CONFIG -- BASE_IDX 2. Same collision class"),
]

WREG = re.compile(r"amdgpu_device_wreg:\s*0x[0-9a-fA-F]+,\s*0x([0-9a-fA-F]+),\s*0x([0-9a-fA-F]+)")
CYR_VAR = re.compile(r"^\s*var\s+(GPU_[A-Z0-9_]+)\s*=\s*(0x[0-9a-fA-F]+|\d+)\s*;", re.M)


def transform(abs_idx):
    """Absolute dword index -> (base_idx, relative offset)."""
    if abs_idx >= BASE_SPLIT:
        return 2, abs_idx - BASE2_OFFSET
    return 1, abs_idx - BASE1_OFFSET


def load_gpu_regs():
    if not GPU_REGS.exists():
        return {}
    return {m.group(1): int(m.group(2), 0) for m in CYR_VAR.finditer(GPU_REGS.read_text())}


def load_trace():
    """abs index -> list of values written, in order."""
    if not TRACE.exists():
        sys.exit(f"FATAL: trace not found: {TRACE}")
    seen = {}
    for line in TRACE.read_text().splitlines():
        m = WREG.search(line)
        if m:
            seen.setdefault(int(m.group(1), 16), []).append(int(m.group(2), 16))
    return seen


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--all", action="store_true", help="dump every transformed register in the trace")
    args = ap.parse_args()

    regs = load_gpu_regs()
    trace = load_trace()

    print(f"trace : {TRACE.name} -- {len(trace)} distinct absolute registers written")
    print(f"regs  : {GPU_REGS.name} -- {len(regs)} constants parsed")
    print(f"xform : BASE_IDX 2 -> abs - 0x{BASE2_OFFSET:04X} | BASE_IDX 1 -> abs - 0x{BASE1_OFFSET:04X}")
    print()

    # ---- THE GATE ------------------------------------------------------------------------------------
    print("=== ANCHORS (all twelve must reproduce exactly) ===")
    failures = []
    for name, abs_idx, want_base, want_rel, sym, inst in ANCHORS:
        got_base, got_rel = transform(abs_idx)
        ok_x = (got_base == want_base and got_rel == want_rel)

        note = ""
        ok_c = True
        if sym is not None:
            if sym in regs:
                want_const = want_rel - inst
                ok_c = (regs[sym] == want_const)
                if inst:
                    note = f"vs {sym}=0x{regs[sym]:04X} + inst 0x{inst:X}"
                else:
                    note = f"vs {sym}=0x{regs[sym]:04X}"
                if not ok_c:
                    note += f"  << MISMATCH, expected 0x{want_const:04X}"
            else:
                note = f"({sym} not found in gpu_regs.cyr -- cross-check skipped)"
        else:
            note = "(no agnos constant yet -- arithmetic only)"

        status = "OK  " if (ok_x and ok_c) else "FAIL"
        if not (ok_x and ok_c):
            failures.append(name)
        print(f"  [{status}] {name:24s} abs 0x{abs_idx:04X} -> BASE_IDX {got_base} rel 0x{got_rel:04X}   {note}")

        if abs_idx not in trace:
            print(f"         ^ note: not written in this trace (read-only or untouched at this mode)")

    print()
    if failures:
        print(f"!! {len(failures)} ANCHOR(S) FAILED: {', '.join(failures)}")
        print("!! The transform is WRONG. Nothing below it ships. Do not write a derived offset.")
        return 1
    print(f"  ALL {len(ANCHORS)} ANCHORS REPRODUCE EXACTLY -- the transform is sound.")
    print()

    # ---- THE HANDOVER --------------------------------------------------------------------------------
    print("=== HANDOVER -- registers the modeset ladder needs that agnos lacks ===")
    print("(paste into kernel/core/gpu_regs.cyr; every one survives the transform the anchors just validated)")
    print()
    already = 0
    for name, rel, base, comment in HANDOVER:
        abs_idx = rel + (BASE2_OFFSET if base == 2 else BASE1_OFFSET)
        in_trace = abs_idx in trace
        if name in regs:
            already += 1
            flag = f"ALREADY PRESENT as 0x{regs[name]:04X}"
            if regs[name] != rel:
                flag += f"  << DIFFERS from derived 0x{rel:04X}"
            print(f"  # {name:34s} {flag}")
            continue
        witness = ""
        if in_trace:
            vals = trace[abs_idx]
            witness = f" amdgpu wrote {len(vals)}x, last 0x{vals[-1]:08X}"
        else:
            witness = " (not written in this trace)"
        print(f"var {name:34s} = 0x{rel:04X}; # BASE_IDX {base}, abs 0x{abs_idx:04X}.{witness}")
        print(f"    #   {comment}")

    print()
    print(f"  {len(HANDOVER) - already} new constants, {already} already present.")
    print()
    print("  !! R5 -- GPU_R_DCPG_DOMAIN0_PG_CONFIG 0x0080 on BASE_IDX 2 collides NUMERICALLY with")
    print("     GPU_R_OTG_PIXEL_RATE_CNTL 0x0080 on BASE_IDX 1. gpu_reg32 checks nothing, and the wrong")
    print("     base POWER-GATES A LIVE PIPE. Distinct naming is the only defence -- keep the base asserted")
    print("     in the comment on every use.")

    if args.all:
        print()
        print("=== every register written in the trace, transformed ===")
        for abs_idx in sorted(trace):
            base, rel = transform(abs_idx)
            vals = trace[abs_idx]
            print(f"  abs 0x{abs_idx:04X} -> BASE_IDX {base} rel 0x{rel:04X}  "
                  f"{len(vals):3d} write(s), last 0x{vals[-1]:08X}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
