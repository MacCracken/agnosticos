#!/usr/bin/env python3
"""H6 — generate the ATOM DIV32/MUL32 sweep vectors and their ORACLE-derived expected values.

WHY
---
`atom_div64_32` / `atom_mul32_split` in agnos `kernel/core/atom.cyr` were rewritten at 1.55.23 -- because
Cyrius `/` is SIGNED and the oracle's `val64 // src` diverges once the dividend's high half has its MSB set
-- and **had never been exercised in any mode, on iron or otherwise**. `SetPixelClock` #12, which the modeset
ladder's M7 rung runs, uses `DIV32_WS` x3 and `MUL32_WS` x3. So this stops being latent and becomes the
pixel-clock divider math.

This script emits the expected values from the SAME arithmetic the Python oracle uses, so the table pasted
into atom.cyr is oracle-derived rather than hand-computed. Quoted verbatim from
`scripts/atom-interp.py` (op_div32 :763-774, op_mul32 :848-854):

    op_div32:  val64 = (dst | (self.divmul[1] << 32)) & 0xFFFFFFFFFFFFFFFF
               q = val64 // src
               self.divmul[0] = q & M32
               self.divmul[1] = (q >> 32) & M32
               (src == 0 -> both 0)

    op_mul32:  val64 = (dst * src) & 0xFFFFFFFFFFFFFFFF
               self.divmul[0] = val64 & M32
               self.divmul[1] = (val64 >> 32) & M32

Note DIV32 DISCARDS the remainder (amdgpu's do_div result is not stored) -- only `atom_op_div`, the 32-bit
DIV, keeps one. Any "fix" that stores a DIV32 remainder is a divergence, not an improvement.

Usage:  python3 scripts/atom-math-vectors.py          # emit the Cyrius selftest body
"""

M32 = 0xFFFFFFFF
M64 = 0xFFFFFFFFFFFFFFFF


def oracle_div32(hi, lo, src):
    if src == 0:
        return 0, 0
    val64 = (lo | (hi << 32)) & M64
    q = val64 // src
    return q & M32, (q >> 32) & M32


def oracle_mul32(dst, src):
    val64 = (dst * src) & M64
    return val64 & M32, (val64 >> 32) & M32


# (hi, lo, src, why) -- the sweep the plan asks for: MSB-set divisors, MSB-set dividends, zero divisors,
# and the 64-bit boundary.
DIV_CASES = [
    (0x00000000, 0x00000000, 0x00000001, "identity zero"),
    (0x00000000, 0x00000064, 0x00000007, "small / small, non-zero remainder"),
    (0x00000000, 0xFFFFFFFF, 0x00000001, "max u32 / 1"),
    (0x00000000, 0xFFFFFFFF, 0xFFFFFFFF, "max / max = 1"),
    (0x00000001, 0x00000000, 0x00000002, "2^32 / 2 -- crosses the halves"),
    (0x80000000, 0x00000000, 0x00000002, "MSB-SET dividend high half -- the signed-divide trap"),
    (0x80000000, 0x00000001, 0x00000003, "MSB-set dividend, odd divisor"),
    (0xFFFFFFFF, 0xFFFFFFFF, 0x00000001, "2^64-1 / 1 -- the 64-bit boundary"),
    (0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, "2^64-1 / 2^32-1 -- quotient exceeds 32 bits"),
    (0xFFFFFFFF, 0xFFFFFFFF, 0x00000002, "2^64-1 / 2"),
    (0x00000000, 0x00003039, 0x80000000, "MSB-SET divisor, quotient 0"),
    (0x7FFFFFFF, 0xFFFFFFFF, 0x80000000, "MSB-set divisor at the sign boundary"),
    (0x12345678, 0x9ABCDEF0, 0xDEADBEEF, "MSB-set divisor, arbitrary dividend"),
    (0x00000000, 0x0003AF5C, 0x0000E1F5, "pixel-clock-shaped: 241500 / 57845"),
    (0x00000001, 0x2A05F200, 0x0003AF5C, "5e9 / 241500 -- #12-shaped divider math"),
    (0x00000000, 0x00000064, 0x00000000, "ZERO DIVISOR -> both halves 0"),
    (0x80000000, 0x00000001, 0x00000000, "ZERO DIVISOR with MSB-set dividend"),
]

# (dst, src, why)
MUL_CASES = [
    (0x00000000, 0x00000000, "zero"),
    (0x00000001, 0x00000001, "identity"),
    (0x00000002, 0x00000003, "small"),
    (0x00010000, 0x00010000, "2^16 x 2^16 = exactly 2^32"),
    (0x80000000, 0x00000002, "2^31 x 2 = exactly 2^32"),
    (0x80000000, 0x80000000, "2^31 x 2^31 = 2^62"),
    (0xFFFFFFFF, 0x00000002, "max x 2"),
    (0xFFFFFFFF, 0xFFFFFFFF, "THE BOUNDARY: 2^64-2^33+1, wraps i64 NEGATIVE"),
    (0xFFFFFFFF, 0x80000000, "max x 2^31 -- also past 2^63"),
    (0x0003AF5C, 0x000003E8, "241500 x 1000 -- pixel-clock-shaped"),
]


def main():
    out = []
    out.append("    # --- DIV32: unsigned (hi:lo) / src, quotient split across divmul0/divmul1 ---")
    for i, (hi, lo, src, why) in enumerate(DIV_CASES):
        q0, q1 = oracle_div32(hi, lo, src)
        out.append(
            f"    atom_div64_32(0x{hi:X}, 0x{lo:X}, 0x{src:X});"
            f"  atom_h6_case({i + 1}, 0x{q0:X}, 0x{q1:X});   # {why}"
        )
    out.append("")
    out.append("    # --- MUL32: 64-bit product split across divmul0 (low) / divmul1 (high) ---")
    base = len(DIV_CASES)
    for i, (dst, src, why) in enumerate(MUL_CASES):
        p0, p1 = oracle_mul32(dst, src)
        out.append(
            f"    atom_mul32_split(0x{dst:X}, 0x{src:X});"
            f"  atom_h6_case({base + i + 1}, 0x{p0:X}, 0x{p1:X});   # {why}"
        )

    print("\n".join(out))
    print()
    print(f"# {len(DIV_CASES)} DIV32 + {len(MUL_CASES)} MUL32 = {len(DIV_CASES) + len(MUL_CASES)} cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
