#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# l1-selftest.py — the regression gate for l1.py. NO ROOT, NO HARDWARE, runs anywhere.
#
# Run this before booking a session, and after ANY edit to l1.py.
#
# ⛔ IT NEGATIVE-CONTROLS THE WRITE-SET ASSERTIONS, and that is the point of the file.
# The five assertions are what make "P and Q are the same program in a different order" a
# PROVEN claim rather than an asserted one — so an assertion that cannot fail is worse than no
# assertion, because it reads as evidence. Each check below perturbs exactly one thing and
# requires the matching assertion to go red. This arc has already been bitten by the failure
# this guards: ATOM_DRY, where "dry" and "live" compiled BYTE-IDENTICAL and two runs differed
# in name but not in behaviour.
import importlib.util, os, struct, sys

# ⛔ THIS SCRIPT TAKES NO ARGUMENTS, AND SAYS SO RATHER THAN IGNORING THEM.
# Silently accepting an unknown flag is exactly the defect that let a typo'd --morph-dig-flip
# run a different experiment than the operator asked for. A tool that ignores your argument is
# a tool that lies about what it just did.
if len(sys.argv) > 1:
    print(f"l1-selftest.py takes NO arguments (got: {' '.join(sys.argv[1:])})")
    print()
    print("⚠ This script is PURE LOGIC VERIFICATION. It touches NO hardware and MAKES NO SOUND.")
    print("  Hearing nothing from it is not a result about the audio path.")
    print()
    print("You probably want l1.py, which is where those flags live:")
    print("    python3 scripts/l1.py --plan-diff     prove P and Q differ only in unmute position")
    print("    python3 scripts/l1.py --dry-run       the above plus the ATOM pre-flight")
    print("    python3 scripts/l1.py --gates         what would refuse this boot, and why")
    print("    python3 scripts/l1.py --boot-entry    print the additive L1 boot entry")
    sys.exit(2)

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("l1", f"{HERE}/l1.py")
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)

print("=" * 78)
print(" L1 SELFTEST — logic only. NO hardware is touched and NO sound is produced.")
print(" This verifies the EXPERIMENT's construction, not the audio path. Silence is correct.")
print("=" * 78)

fails = []
def check(name, cond, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {name}" + (f"  {detail}" if detail and not cond else ""))
    if not cond: fails.append(name)

def section(t): print(f"\n=== {t} ===")

section("ANCHORS (twin has not drifted from agnos)")
check("all anchored literals still present in agnos", not m.check_anchors(), str(m.check_anchors()))

c = m.Consts()
section("CONSTANT SOURCING")
check("constants extracted from gpu_regs.cyr", len(c.vals) > 500, f"{len(c.vals)}")
try:
    c["GPU_NOT_A_REAL_CONSTANT"]; ok = False
except KeyError:
    ok = True
check("an undefined name REFUSES rather than defaulting", ok)

section("WRITE-SET ASSERTIONS — positive")
plans = m.build_all_plans(c)
res = m.write_set_assertions(plans)
for r in res:
    check(r["assertion"], r["pass"], str(r["detail"]))

section("WRITE-SET ASSERTIONS — NEGATIVE CONTROLS (each MUST fail)")
def assertion(plans, prefix):
    return [r for r in m.write_set_assertions(plans) if r["assertion"].startswith(prefix)][0]
p = m.build_all_plans(c); p["Q"].ops[p["Q"].blocks["STAGE"][0]].set_ ^= 0x40
check("A1 fails when Q's staging moves by ONE bit", not assertion(p, "A1")["pass"])
check("A3 fails when Q's staging moves by ONE bit", not assertion(p, "A3")["pass"])
p = m.build_all_plans(c); p["P"].ops[p["P"].blocks["U_PRE"][0]].set_ ^= 0x1
check("A2 fails when P's unmute moves by ONE bit", not assertion(p, "A2")["pass"])
p = m.build_all_plans(c); p["P"] = p["Q"]
check("A4 fails when P is made identical to Q", not assertion(p, "A4")["pass"])
p = m.build_all_plans(c); p["N0"].ops.append(m.Op("RMW", "DIG_BE_CNTL", rel=0, set_=1))
check("A5 fails when the flat null touches DIG_BE_CNTL", not assertion(p, "A5")["pass"])

section("FORBIDDEN-VALUE SCAN")
p = m.build_all_plans(c)
check("clean on every derived arm",
      not any(p[a].check_forbidden(c) for a in ("N0", "N1", "P", "Q", "D")))
p["Q"].ops.append(m.Op("W", "AFMT_GENERIC_0", rel=0, value=0x80885E8F))
check("catches amdgpu's YCbCr AVI body injected into a derived arm",
      bool(p["Q"].check_forbidden(c)))

section("P vs Q — position is the ONLY difference")
p = m.build_all_plans(c)
d = m.diff_positions(p["P"], p["Q"])
check("non-unmute op sequence identical", any("identical : YES" in x for x in d), str(d))

section("REPORT GRAMMAR")
check("bare 0 = nothing", m.parse_report("0")["grade"] == 0)
check("full code parses", m.parse_report("34CUL<")["bursts"] == "4")
check("garbage rejected", m.parse_report("xyz") is None)
check("short code rejected", m.parse_report("34CU") is None)
check("out-of-range grade rejected", m.parse_report("44CUL<") is None)
check("U-phase 7th char", m.parse_report("34CUL<S", u_phase=True)["temporal"] == "S")

section("SCORING (pre-registered: bursts AND channels ONLY)")
pat = {"bursts": 4, "channels": "L"}
check("exact match scores", m.score_window(m.parse_report("34CUL<"), pat))
check("wrong PITCH still matches — a degraded-but-real tone must not read as a negative",
      m.score_window(m.parse_report("34AUL<"), pat))
check("wrong bursts does not", not m.score_window(m.parse_report("33CUL<"), pat))
check("wrong channels does not", not m.score_window(m.parse_report("34CUR<"), pat))

section("CLASSIFY")
cl = m.classify({"a": [{"grade": 3, "match": True}] * 2,
                 "b": [{"grade": 0, "match": False}] * 2,
                 "c": [{"grade": 1, "match": False}] * 2,
                 "d": [{"grade": 3, "match": True}, {"grade": 0, "match": False}]})
check("2 verified strong = sound", cl["a"] == "sound")
check("all zero = silent", cl["b"] == "silent")
check("grade-1 NOISE FLOOR counts as silent, never as a positive", cl["c"] == "silent")
check("mixed = mixed (INCONCLUSIVE, not rounded)", cl["d"] == "mixed")

section("VERDICT LATTICE")
G = {"V1": True}
check("Rlit silent => VOID-HARNESS, ahead of every other branch",
      m.verdict({"Rlit": "silent", "P": "sound", "Q": "sound"}, G)[0] == "VOID-HARNESS")
check("P silent + Q sound + D sound => SEQUENCING IS LIVE",
      m.verdict({"Rlit": "sound", "P": "silent", "Q": "sound", "D": "sound"}, G)[0].startswith("A —"))
check("both sound => UNCONFIRMED",
      m.verdict({"Rlit": "sound", "P": "sound", "Q": "sound"}, G)[0].startswith("UNCONFIRMED"))
check("both agnos arms silent + ref sounds => PROGRAM DELTA",
      m.verdict({"Rlit": "sound", "P": "silent", "Q": "silent"}, G)[0] == "PROGRAM DELTA")
check("a failed gate VOIDs even a 'good' shape",
      m.verdict({"Rlit": "sound", "P": "silent", "Q": "sound", "D": "sound"},
                {"V1": False})[0] == "VOID")

section("KEY DERIVATION")
k1 = m.derive_key("abc", 14, ["P"] * 14)
check("deterministic — re-derivable from (seed, script)", k1 == m.derive_key("abc", 14, ["P"] * 14))
check("different master => different plan", m.derive_key("abd", 14, ["P"] * 14) != k1)
banned = [w for w in m.derive_key("abc", 400, ["P"] * 400)
          if w["pattern"]["bursts"] == 3 and w["pattern"]["pitch_hz"] == 880
          and w["pattern"]["sweep"] == "U" and tuple(w["pattern"]["span"]) == (300, 3000)]
check("the PUBLISHED 2026-07-15 pattern is never drawn", not banned, f"drawn {len(banned)}x")

section("STIMULUS")
pcm = m.render_pattern({"bursts": 3, "pitch_hz": 440, "sweep": "U", "span": (300, 3000),
                        "channels": "B", "gap_ms": 120})
check("6 s stereo 16-bit @ 48 kHz", len(pcm) == 48000 * 6 * 4, f"{len(pcm)} B")
peak = max(abs(struct.unpack_from("<h", pcm, i)[0]) for i in range(0, len(pcm), 400))
check("near-full-scale — level identical in every window, so a level report means the PATH",
      peak > 25000, f"peak {peak}")
lo = m.render_pattern({"bursts": 2, "pitch_hz": 440, "sweep": "U", "span": (300, 3000),
                       "channels": "L", "gap_ms": 120})
check("L-only is genuinely silent on R",
      max(abs(struct.unpack_from("<h", lo, i + 2)[0]) for i in range(0, len(lo), 4)) == 0)

section("ARM BUDGET")
check("Block 1 is 14 windows", sum(s["reps"] for s in m.ARM_SPECS.values()) == 14,
      str(sum(s["reps"] for s in m.ARM_SPECS.values())))

print("\n" + "=" * 78)
print(" L1 SELFTEST: " + ("ALL PASS" if not fails else f"{len(fails)} FAILED -> {fails}"))
print("=" * 78)
sys.exit(1 if fails else 0)
