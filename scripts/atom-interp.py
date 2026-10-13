#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# atom-interp.py — a FAITHFUL Python port of AMD's AtomBIOS command-table
# interpreter (drivers/gpu/drm/amd/amdgpu/atom.c), built to drive the HDMI
# transmitter bring-up on a real Cezanne/Renoir DCN 2.1 GPU.
#
# This is the A1 bite of AGNOS's HDMI-modeset arc: prove the interpreter on
# Linux (against the real BAR5) BEFORE porting it to the Cyrius kernel. A wrong
# operand decode here would hang the machine on --live, so every operand size,
# mask, shift, and pointer advance is a line-for-line port of atom.c. The
# functions ported (and the atom.c line they mirror) are cited inline.
#
# REFERENCE (torvalds/linux master, fetched verbatim, GPL/MIT):
#   drivers/gpu/drm/amd/amdgpu/atom.c   — the interpreter
#   drivers/gpu/drm/amd/amdgpu/atom.h   — opcode/const #defines
#   drivers/gpu/drm/amd/include/atomfirmware.h — the param structs
#
# PORTED (atom.c symbol -> this file):
#   atom_iio_execute            -> Atom.iio_execute
#   atom_get_src_int            -> Atom.get_src_int
#   atom_skip_src_int           -> Atom.skip_src_int
#   atom_get_src                -> Atom.get_src
#   atom_get_src_direct         -> Atom.get_src_direct
#   atom_get_dst                -> Atom.get_dst
#   atom_skip_dst               -> Atom.skip_dst
#   atom_put_dst                -> Atom.put_dst
#   atom_op_*  (all 41 kinds)   -> Atom.op_*
#   opcode_table[]              -> OPCODE_TABLE
#   amdgpu_atom_execute_table_locked -> Atom.execute_table_locked
#   amdgpu_atom_execute_table   -> Atom.execute_table
#   atom_index_iio              -> Atom.index_iio
#   ROM/master-table parse (amdgpu_atom_parse) -> Atom.__init__
#
# DEFAULT IS --dry-run: every REG/PLL/MC read+write and every DELAY is logged,
# reads return 0 (or a value from --regsnapshot), and NOTHING touches hardware.
# --live mmaps BAR5 and does real MMIO (the operator runs that on the target).
#
# Usage:
#   ./atom-interp.py                       # dry-run DIG1TransmitterControl(ENABLE,HDMI)
#   ./atom-interp.py --command transmitter --action enable
#   ./atom-interp.py --command encoder-setup
#   ./atom-interp.py --histogram-only      # just prove the stepper on both tables
#   ./atom-interp.py --live ...            # OPERATOR ONLY — real BAR5 writes

import argparse
import os
import struct
import sys
import time

# ============================================================================
#  Constants  — verbatim from atom.h and the top of atom.c
# ============================================================================

# ---- atom.h: ROM / master-table layout ----
ATOM_ROM_TABLE_PTR = 0x48          # bios[0x48] -> ATOM_ROM_HEADER offset
ATOM_ROM_MAGIC_PTR = 4
ATOM_ROM_MSG_PTR   = 0x10
ATOM_ROM_CMD_PTR   = 0x1E          # rom_header + 0x1E -> MasterCommandTable off
ATOM_ROM_DATA_PTR  = 0x20          # rom_header + 0x20 -> MasterDataTable off
ATOM_DATA_IIO_PTR  = 0x32          # data_table + 0x32 -> IndirectIOAccess ptr

# ---- atom.h: command-table header field offsets ----
ATOM_CT_SIZE_PTR = 0               # U16 struct size
ATOM_CT_WS_PTR   = 4               # U8  workspace size (dwords)
ATOM_CT_PS_PTR   = 5               # U8  param space size (& mask)
ATOM_CT_PS_MASK  = 0x7F
ATOM_CT_CODE_PTR = 6               # bytecode starts here

ATOM_OP_CNT = 127
ATOM_OP_EOT = 91

ATOM_CASE_MAGIC = 0x63
ATOM_CASE_END   = 0x5A5A

# ---- atom.h: ATOM_ARG_* (operand address space, attr & 7) ----
ATOM_ARG_REG = 0
ATOM_ARG_PS  = 1
ATOM_ARG_WS  = 2
ATOM_ARG_FB  = 3
ATOM_ARG_ID  = 4
ATOM_ARG_IMM = 5
ATOM_ARG_PLL = 6
ATOM_ARG_MC  = 7

# ---- atom.h: ATOM_SRC_* (align field, (attr>>3) & 7) ----
ATOM_SRC_DWORD  = 0
ATOM_SRC_WORD0  = 1
ATOM_SRC_WORD8  = 2
ATOM_SRC_WORD16 = 3
ATOM_SRC_BYTE0  = 4
ATOM_SRC_BYTE8  = 5
ATOM_SRC_BYTE16 = 6
ATOM_SRC_BYTE24 = 7

# ---- atom.h: workspace special addresses (WS idx) ----
ATOM_WS_QUOTIENT  = 0x40
ATOM_WS_REMAINDER = 0x41
ATOM_WS_DATAPTR   = 0x42
ATOM_WS_SHIFT     = 0x43
ATOM_WS_OR_MASK   = 0x44
ATOM_WS_AND_MASK  = 0x45
ATOM_WS_FB_WINDOW = 0x46
ATOM_WS_ATTRIBUTES = 0x47
ATOM_WS_REGPTR    = 0x48

# ---- atom.h: IIO opcodes + lengths ----
ATOM_IIO_NOP        = 0
ATOM_IIO_START      = 1
ATOM_IIO_READ       = 2
ATOM_IIO_WRITE      = 3
ATOM_IIO_CLEAR      = 4
ATOM_IIO_SET        = 5
ATOM_IIO_MOVE_INDEX = 6
ATOM_IIO_MOVE_ATTR  = 7
ATOM_IIO_MOVE_DATA  = 8
ATOM_IIO_END        = 9
# atom.c:1323  static int atom_iio_len[] = { 1, 2, 3, 3, 3, 3, 4, 4, 4, 3 };
ATOM_IIO_LEN = [1, 2, 3, 3, 3, 3, 4, 4, 4, 3]

# ---- atom.h: io_mode ----
ATOM_IO_MM    = 0
ATOM_IO_PCI   = 1
ATOM_IO_SYSIO = 2
ATOM_IO_IIO   = 0x80

# ---- atom.c: conditions / ports / units ----
ATOM_COND_ABOVE        = 0
ATOM_COND_ABOVEOREQUAL = 1
ATOM_COND_ALWAYS       = 2
ATOM_COND_BELOW        = 3
ATOM_COND_BELOWOREQUAL = 4
ATOM_COND_EQUAL        = 5
ATOM_COND_NOTEQUAL     = 6

ATOM_PORT_ATI   = 0
ATOM_PORT_PCI   = 1
ATOM_PORT_SYSIO = 2

ATOM_UNIT_MICROSEC = 0
ATOM_UNIT_MILLISEC = 1

ATOM_CMD_TIMEOUT_SEC  = 20
ATOM_EXECUTE_MAX_DEPTH = 32

# ---- atom.c:80  the three operand-decode lookup tables (verbatim) ----
atom_arg_mask = [0xFFFFFFFF, 0xFFFF, 0xFFFF00, 0xFFFF0000, 0xFF, 0xFF00, 0xFF0000, 0xFF000000]
atom_arg_shift = [0, 0, 8, 16, 0, 8, 16, 24]
atom_dst_to_src = [
    [0, 0, 0, 0],
    [1, 2, 3, 0],
    [1, 2, 3, 0],
    [1, 2, 3, 0],
    [4, 5, 6, 7],
    [4, 5, 6, 7],
    [4, 5, 6, 7],
    [4, 5, 6, 7],
]
atom_def_dst = [0, 0, 1, 2, 0, 1, 2, 3]

M32 = 0xFFFFFFFF


def u32(v):
    return v & M32


# ============================================================================
#  atomfirmware.h enums used to build the param structs
# ============================================================================

# enum atom_encode_mode_def
ATOM_ENCODER_MODE_DP    = 0
ATOM_ENCODER_MODE_LVDS  = 1
ATOM_ENCODER_MODE_DVI   = 2
ATOM_ENCODER_MODE_HDMI  = 3
ATOM_ENCODER_MODE_DP_MST = 5
ATOM_ENCODER_MODE_CRT   = 15

# enum atom_dig_transmitter_control_action
ATOM_TRANSMITTER_ACTION_DISABLE        = 0
ATOM_TRANSMITTER_ACTION_ENABLE         = 1
ATOM_TRANSMITTER_ACTION_BL_BRIGHTNESS  = 4
ATOM_TRANSMITTER_ACTION_INIT           = 7
ATOM_TRANSMITTER_ACTION_DISABLE_OUTPUT = 8
ATOM_TRANSMITTER_ACTION_ENABLE_OUTPUT  = 9
ATOM_TRANSMITTER_ACTION_SETUP          = 10
ATOM_TRANSMITTER_ACTION_SETUP_VSEMPH   = 11
ATOM_TRANSMITTER_ACTION_POWER_ON       = 12
ATOM_TRANSMITTER_ACTION_POWER_OFF      = 13

# enum atom_dig_encoder_control_action
ATOM_ENCODER_CMD_DISABLE_DIG   = 0
ATOM_ENCODER_CMD_ENABLE_DIG    = 1
ATOM_ENCODER_CMD_STREAM_SETUP  = 0x0F
ATOM_ENCODER_CMD_LINK_SETUP    = 0x11
ATOM_ENCODER_CMD_ENCODER_BLANK = 0x12

# enum atom_dig_encoder_control_v5_digid  (DIGx front-end selector)
ATOM_ENCODER_CONFIG_V5_DIG1_ENCODER = 0x01

# The four command-table indices we care about (from the parsed VBIOS).
CMD_DIGxEncoderControl    = 4
CMD_SetPixelClock         = 12
CMD_EnableDispPowerGating = 13
CMD_DIG1TransmitterControl = 76


# ============================================================================
#  Register backend — TWO modes. DEFAULT is dry-run (no hardware touch).
# ============================================================================
class DryRunCard:
    """card_info equivalent. reg/pll/mc read+write are logged and never touch
    hardware. reads return 0 unless a snapshot supplies a value.

    atom.c passes reg_read/reg_write a *dword register index* (idx, already
    += reg_block). amdgpu's RREG32(idx) accesses BAR5 at byte offset idx*4
    (readl(rmmio + idx*4)); a poke tool would touch resource5 at idx*4. We
    record both so the trace is directly comparable to a BAR5 dump.
    """

    def __init__(self, tracer, snapshot=None):
        self.tr = tracer
        self.snap = snapshot or {}   # {dword_idx: value}

    def reg_read(self, idx):
        val = self.snap.get(idx, 0) & M32
        self.tr.access("REG", "R", idx, val, byte_off=idx * 4)
        return val

    def reg_write(self, idx, val):
        self.tr.access("REG", "W", idx, val & M32, byte_off=idx * 4)

    def pll_read(self, idx):
        val = self.snap.get(("PLL", idx), 0) & M32
        self.tr.access("PLL", "R", idx, val)
        return val

    def pll_write(self, idx, val):
        self.tr.access("PLL", "W", idx, val & M32)

    def mc_read(self, idx):
        val = self.snap.get(("MC", idx), 0) & M32
        self.tr.access("MC", "R", idx, val)
        return val

    def mc_write(self, idx, val):
        self.tr.access("MC", "W", idx, val & M32)

    def delay(self, count, unit):
        self.tr.delay(count, unit)


class LiveCard:
    """OPERATOR ONLY. Real BAR5 MMIO, poke-tool style. Never exercised in this
    genesis-repo bring-up; the operator runs --live on archaemenid."""

    def __init__(self, tracer, resource_path):
        import mmap
        self.tr = tracer
        self.fd = os.open(resource_path, os.O_RDWR | os.O_SYNC)
        self.size = os.fstat(self.fd).st_size
        self.mm = mmap.mmap(self.fd, self.size, mmap.MAP_SHARED,
                            mmap.PROT_READ | mmap.PROT_WRITE)

    def _bounds(self, idx):
        if idx * 4 + 4 > self.size:
            raise IndexError(f"REG idx 0x{idx:x} (byte 0x{idx*4:x}) beyond BAR5 size 0x{self.size:x}")

    def reg_read(self, idx):
        self._bounds(idx)
        val = struct.unpack_from("<I", self.mm, idx * 4)[0]
        self.tr.access("REG", "R", idx, val, byte_off=idx * 4)
        return val

    def reg_write(self, idx, val):
        self._bounds(idx)
        self.tr.access("REG", "W", idx, val & M32, byte_off=idx * 4)
        struct.pack_into("<I", self.mm, idx * 4, val & M32)

    # PLL/MC on modern DCN are not wired through raw BAR5; the operator supplies
    # these if a table needs them. Left as explicit failures rather than silent 0.
    def pll_read(self, idx):
        raise NotImplementedError("live PLL read not wired (no table under test needs it)")

    def pll_write(self, idx, val):
        raise NotImplementedError("live PLL write not wired")

    def mc_read(self, idx):
        raise NotImplementedError("live MC read not wired")

    def mc_write(self, idx, val):
        raise NotImplementedError("live MC write not wired")

    def delay(self, count, unit):
        self.tr.delay(count, unit)
        if unit == ATOM_UNIT_MICROSEC:
            time.sleep(count / 1_000_000.0)
        else:
            time.sleep(count / 1000.0)


# ============================================================================
#  Tracer — ordered trace of every access + branch + delay, and a histogram.
# ============================================================================
class Tracer:
    def __init__(self, verbose=True):
        self.verbose = verbose
        self.events = []          # ordered list of (kind, detail)
        self.hist = {}            # opcode-name -> count
        self.depth = 0
        self.n_reg_r = self.n_reg_w = 0
        self.n_delay = 0

    def _emit(self, line):
        if self.verbose:
            print(line)

    def opcode(self, name, addr):
        self.hist[name] = self.hist.get(name, 0) + 1

    def step(self, name, addr):
        self.events.append(("op", (self.depth, name, addr)))
        self._emit(f"{'  ' * self.depth}{name} @ 0x{addr:04X}")

    def access(self, space, rw, idx, val, byte_off=None):
        if space == "REG":
            if rw == "R":
                self.n_reg_r += 1
            else:
                self.n_reg_w += 1
        self.events.append(("acc", (self.depth, space, rw, idx, val, byte_off)))
        off = f" (BAR5+0x{byte_off:05X})" if byte_off is not None else ""
        self._emit(f"{'  ' * (self.depth + 1)}{space} {rw} [0x{idx:04X}]{off} "
                   f"{'->' if rw == 'R' else '<-'} 0x{val:08X}")

    def delay(self, count, unit):
        self.n_delay += 1
        u = "us" if unit == ATOM_UNIT_MICROSEC else "ms"
        self.events.append(("delay", (self.depth, count, unit)))
        self._emit(f"{'  ' * (self.depth + 1)}DELAY {count} {u}")

    def branch(self, text):
        self.events.append(("branch", (self.depth, text)))
        self._emit(f"{'  ' * (self.depth + 1)}{text}")

    def note(self, text):
        self.events.append(("note", (self.depth, text)))
        self._emit(f"{'  ' * (self.depth + 1)}{text}")


# ============================================================================
#  The exec context (atom.c: typedef struct { ... } atom_exec_context)
# ============================================================================
class ExecContext:
    __slots__ = ("ps", "ps_byte_off", "ps_size", "ws", "ws_size", "ps_shift",
                 "start", "last_jump", "last_jump_t", "abort")

    def __init__(self):
        self.ps = None          # bytearray, indexed as dwords
        self.ps_byte_off = 0    # window base into ps (calltable sub-view)
        self.ps_size = 0        # dwords
        self.ws = None          # list[int], dwords
        self.ws_size = 0
        self.ps_shift = 0
        self.start = 0
        self.last_jump = 0
        self.last_jump_t = 0.0
        self.abort = False


# ============================================================================
#  The interpreter (struct atom_context + the whole engine)
# ============================================================================
class Atom:
    def __init__(self, bios, card, tracer, loop_timeout=2.0, max_steps=2_000_000):
        self.bios = bytearray(bios)
        self.card = card
        self.tr = tracer
        self.loop_timeout = loop_timeout
        self.max_steps = max_steps

        # struct atom_context state
        self.io_mode = ATOM_IO_MM
        self.reg_block = 0
        self.data_block = 0
        self.fb_base = 0
        self.divmul = [0, 0]
        self.shift = 0
        self.io_attr = 0
        self.cs_equal = False
        self.cs_above = False
        self.execute_depth = 0
        self.iio = [0] * 256
        # ⭐ PS-read attestation: which parameter dwords the executed table actually consumed.
        # Lives on Atom, not ExecContext -- ExecContext is __slots__'d and per-invocation, and the
        # question ("did this table read the field I think I packed?") spans the whole run.
        self.ps_read = set()

        # scratch (GPU scratch dwords). Modelled as zeros for dry-run; FB ops
        # are rare in the tables under test.
        self.scratch = [0] * 256
        self.scratch_size_bytes = len(self.scratch) * 4

        # ---- amdgpu_atom_parse: read the ROM header + master tables ----
        self.rom = self.U16(ATOM_ROM_TABLE_PTR)
        magic = bytes(self.bios[self.rom + ATOM_ROM_MAGIC_PTR:
                                self.rom + ATOM_ROM_MAGIC_PTR + 4])
        if magic != b"ATOM":
            raise ValueError(f"not an ATOM rom (magic {magic!r} @ 0x{self.rom:x})")
        self.cmd_table = self.U16(self.rom + ATOM_ROM_CMD_PTR)
        self.data_table = self.U16(self.rom + ATOM_ROM_DATA_PTR)
        self.index_iio()

    # ---- atom-bits.h: U8/U16/U32 == CU8/CU16/CU32 read bios at absolute off ---
    def U8(self, o):
        return self.bios[o]

    def U16(self, o):
        return self.bios[o] | (self.bios[o + 1] << 8)

    def U32(self, o):
        return self.bios[o] | (self.bios[o + 1] << 8) | \
               (self.bios[o + 2] << 16) | (self.bios[o + 3] << 24)

    # atom.c:1325 atom_index_iio
    def index_iio(self):
        base = self.U16(self.data_table + ATOM_DATA_IIO_PTR) + 4
        # guard: this VBIOS ships an empty IndirectIOAccess table; the while
        # simply won't enter. Bounds-guard anyway.
        while 0 <= base < len(self.bios) and self.U8(base) == ATOM_IIO_START:
            self.iio[self.U8(base + 1)] = base + 2
            base += 2
            while self.U8(base) != ATOM_IIO_END:
                base += ATOM_IIO_LEN[self.U8(base)]
            base += 3

    # atom.c:113 atom_iio_execute
    def iio_execute(self, base, index, data):
        temp = 0xCDCDCDCD
        while True:
            op = self.U8(base)
            if op == ATOM_IIO_NOP:
                base += 1
            elif op == ATOM_IIO_READ:
                temp = self.card.reg_read(self.U16(base + 1))
                base += 3
            elif op == ATOM_IIO_WRITE:
                self.card.reg_write(self.U16(base + 1), temp)
                base += 3
            elif op == ATOM_IIO_CLEAR:
                temp &= ~(self._m(self.U8(base + 1)) << self.U8(base + 2))
                temp &= M32
                base += 3
            elif op == ATOM_IIO_SET:
                temp |= (self._m(self.U8(base + 1)) << self.U8(base + 2))
                temp &= M32
                base += 3
            elif op == ATOM_IIO_MOVE_INDEX:
                n, sh, dsh = self.U8(base + 1), self.U8(base + 2), self.U8(base + 3)
                temp &= ~(self._m(n) << dsh) & M32
                temp |= ((index >> sh) & self._m(n)) << dsh
                temp &= M32
                base += 4
            elif op == ATOM_IIO_MOVE_DATA:
                n, sh, dsh = self.U8(base + 1), self.U8(base + 2), self.U8(base + 3)
                temp &= ~(self._m(n) << dsh) & M32
                temp |= ((data >> sh) & self._m(n)) << dsh
                temp &= M32
                base += 4
            elif op == ATOM_IIO_MOVE_ATTR:
                n, sh, dsh = self.U8(base + 1), self.U8(base + 2), self.U8(base + 3)
                temp &= ~(self._m(n) << dsh) & M32
                temp |= ((self.io_attr >> sh) & self._m(n)) << dsh
                temp &= M32
                base += 4
            elif op == ATOM_IIO_END:
                return temp & M32
            else:
                self.tr.note("Unknown IIO opcode")
                return 0

    @staticmethod
    def _m(n):
        # C: (0xFFFFFFFF >> (32 - n)). x86 masks shift to 5 bits, so n==32
        # (=> shift 0) yields 0xFFFFFFFF; n in 1..31 yields the low-n-bit mask.
        s = (32 - n) & 31
        return (0xFFFFFFFF >> s) & M32

    # ------------------------------------------------------------------
    #  atom.c:185  atom_get_src_int
    #  ptr is a 1-element list emulating C's  int *ptr .
    #  saved is a 1-element list or None emulating  uint32_t *saved .
    # ------------------------------------------------------------------
    def get_src_int(self, attr, ptr, saved, prnt):
        val = 0xCDCDCDCD
        arg = attr & 7
        align = (attr >> 3) & 7
        if arg == ATOM_ARG_REG:
            idx = self.U16(ptr[0]); ptr[0] += 2
            idx = (idx + self.reg_block) & M32   # atom.c: uint32_t, no 16-bit trunc
            if self.io_mode == ATOM_IO_MM:
                val = self.card.reg_read(idx)
            elif self.io_mode == ATOM_IO_PCI:
                self.tr.note("PCI registers are not implemented"); return 0
            elif self.io_mode == ATOM_IO_SYSIO:
                self.tr.note("SYSIO registers are not implemented"); return 0
            else:
                if not (self.io_mode & 0x80):
                    self.tr.note("Bad IO mode"); return 0
                if not self.iio[self.io_mode & 0x7F]:
                    self.tr.note(f"Undefined indirect IO read method {self.io_mode & 0x7F}")
                    return 0
                val = self.iio_execute(self.iio[self.io_mode & 0x7F], idx, 0)
        elif arg == ATOM_ARG_PS:
            idx = self.U8(ptr[0]); ptr[0] += 1
            if idx < ctx_ps_size(self):
                # ⭐ ATTEST THE PARAMETER LAYOUT INSTEAD OF ASSUMING IT. Every parameter struct in
                # this tool is transcribed from a header, and a mis-transcribed field is invisible:
                # the table just reads a zero and does something plausible. Recording which PS
                # dwords the bytecode ACTUALLY consumes turns the layout into a measurement. This is
                # the same discipline that made FE_SOURCE_SELECT trustworthy (two iron captures, not
                # a header) and that MD-2's phyid lacked when it stood wrong for six days.
                self.ps_read.add(idx)
                val = ps_dword(self.ectx, idx)
            else:
                self.tr.note(f"PS index out of range: {idx} > {ctx_ps_size(self)}")
        elif arg == ATOM_ARG_WS:
            idx = self.U8(ptr[0]); ptr[0] += 1
            if idx == ATOM_WS_QUOTIENT:
                val = self.divmul[0]
            elif idx == ATOM_WS_REMAINDER:
                val = self.divmul[1]
            elif idx == ATOM_WS_DATAPTR:
                val = self.data_block
            elif idx == ATOM_WS_SHIFT:
                val = self.shift
            elif idx == ATOM_WS_OR_MASK:
                val = (1 << self.shift) & M32
            elif idx == ATOM_WS_AND_MASK:
                val = (~(1 << self.shift)) & M32
            elif idx == ATOM_WS_FB_WINDOW:
                val = self.fb_base
            elif idx == ATOM_WS_ATTRIBUTES:
                val = self.io_attr
            elif idx == ATOM_WS_REGPTR:
                val = self.reg_block
            else:
                if idx < self.ectx.ws_size:
                    val = self.ectx.ws[idx]
                else:
                    self.tr.note(f"WS index out of range: {idx} > {self.ectx.ws_size}")
        elif arg == ATOM_ARG_ID:
            idx = self.U16(ptr[0]); ptr[0] += 2
            val = self.U32(idx + self.data_block)
        elif arg == ATOM_ARG_FB:
            idx = self.U8(ptr[0]); ptr[0] += 1
            if (self.fb_base + (idx * 4)) > self.scratch_size_bytes:
                self.tr.note(f"fb read beyond scratch: {self.fb_base + idx*4} vs {self.scratch_size_bytes}")
                val = 0
            else:
                val = self.scratch[(self.fb_base // 4) + idx]
        elif arg == ATOM_ARG_IMM:
            if align == ATOM_SRC_DWORD:
                val = self.U32(ptr[0]); ptr[0] += 4
                return val & M32
            elif align in (ATOM_SRC_WORD0, ATOM_SRC_WORD8, ATOM_SRC_WORD16):
                val = self.U16(ptr[0]); ptr[0] += 2
                return val & M32
            else:  # BYTE0/8/16/24
                val = self.U8(ptr[0]); ptr[0] += 1
                return val & M32
        elif arg == ATOM_ARG_PLL:
            idx = self.U8(ptr[0]); ptr[0] += 1
            val = self.card.pll_read(idx)
        elif arg == ATOM_ARG_MC:
            idx = self.U8(ptr[0]); ptr[0] += 1
            val = self.card.mc_read(idx)

        val &= M32
        if saved is not None:
            saved[0] = val
        val &= atom_arg_mask[align]
        val = val >> atom_arg_shift[align]
        return val & M32

    # atom.c:376  atom_skip_src_int
    def skip_src_int(self, attr, ptr):
        align = (attr >> 3) & 7
        arg = attr & 7
        if arg in (ATOM_ARG_REG, ATOM_ARG_ID):
            ptr[0] += 2
        elif arg in (ATOM_ARG_PLL, ATOM_ARG_MC, ATOM_ARG_PS, ATOM_ARG_WS, ATOM_ARG_FB):
            ptr[0] += 1
        elif arg == ATOM_ARG_IMM:
            if align == ATOM_SRC_DWORD:
                ptr[0] += 4
            elif align in (ATOM_SRC_WORD0, ATOM_SRC_WORD8, ATOM_SRC_WORD16):
                ptr[0] += 2
            else:
                ptr[0] += 1

    # atom.c:411  atom_get_src
    def get_src(self, attr, ptr):
        return self.get_src_int(attr, ptr, None, 1)

    # atom.c:416  atom_get_src_direct
    def get_src_direct(self, align, ptr):
        if align == ATOM_SRC_DWORD:
            val = self.U32(ptr[0]); ptr[0] += 4
        elif align in (ATOM_SRC_WORD0, ATOM_SRC_WORD8, ATOM_SRC_WORD16):
            val = self.U16(ptr[0]); ptr[0] += 2
        else:
            val = self.U8(ptr[0]); ptr[0] += 1
        return val & M32

    # atom.c:442  atom_get_dst
    def get_dst(self, arg, attr, ptr, saved, prnt):
        new_attr = arg | (atom_dst_to_src[(attr >> 3) & 7][(attr >> 6) & 3] << 3)
        return self.get_src_int(new_attr, ptr, saved, prnt)

    # atom.c:451  atom_skip_dst
    def skip_dst(self, arg, attr, ptr):
        new_attr = arg | (atom_dst_to_src[(attr >> 3) & 7][(attr >> 6) & 3] << 3)
        self.skip_src_int(new_attr, ptr)

    # atom.c:458  atom_put_dst
    def put_dst(self, arg, attr, ptr, val, saved):
        align = atom_dst_to_src[(attr >> 3) & 7][(attr >> 6) & 3]
        val = u32(val)
        val = (val << atom_arg_shift[align]) & M32
        val &= atom_arg_mask[align]
        saved = saved & (~atom_arg_mask[align] & M32)
        val |= saved
        val &= M32
        if arg == ATOM_ARG_REG:
            idx = self.U16(ptr[0]); ptr[0] += 2
            idx = (idx + self.reg_block) & M32   # atom.c: uint32_t, no 16-bit trunc
            if self.io_mode == ATOM_IO_MM:
                if idx == 0:
                    self.card.reg_write(idx, (val << 2) & M32)
                else:
                    self.card.reg_write(idx, val)
            elif self.io_mode == ATOM_IO_PCI:
                self.tr.note("PCI registers are not implemented"); return
            elif self.io_mode == ATOM_IO_SYSIO:
                self.tr.note("SYSIO registers are not implemented"); return
            else:
                if not (self.io_mode & 0x80):
                    self.tr.note("Bad IO mode"); return
                if not self.iio[self.io_mode & 0xFF]:
                    self.tr.note(f"Undefined indirect IO write method {self.io_mode & 0x7F}")
                    return
                self.iio_execute(self.iio[self.io_mode & 0xFF], idx, val)
        elif arg == ATOM_ARG_PS:
            idx = self.U8(ptr[0]); ptr[0] += 1
            if idx >= self.ectx.ps_size:
                self.tr.note(f"PS index out of range: {idx} > {self.ectx.ps_size}")
                return
            set_ps_dword(self.ectx, idx, val)
        elif arg == ATOM_ARG_WS:
            idx = self.U8(ptr[0]); ptr[0] += 1
            if idx == ATOM_WS_QUOTIENT:
                self.divmul[0] = val
            elif idx == ATOM_WS_REMAINDER:
                self.divmul[1] = val
            elif idx == ATOM_WS_DATAPTR:
                self.data_block = val
            elif idx == ATOM_WS_SHIFT:
                self.shift = val
            elif idx in (ATOM_WS_OR_MASK, ATOM_WS_AND_MASK):
                pass
            elif idx == ATOM_WS_FB_WINDOW:
                self.fb_base = val
            elif idx == ATOM_WS_ATTRIBUTES:
                self.io_attr = val
            elif idx == ATOM_WS_REGPTR:
                self.reg_block = val
            else:
                if idx >= self.ectx.ws_size:
                    self.tr.note(f"WS index out of range: {idx} > {self.ectx.ws_size}")
                    return
                self.ectx.ws[idx] = val
        elif arg == ATOM_ARG_FB:
            idx = self.U8(ptr[0]); ptr[0] += 1
            if (self.fb_base + (idx * 4)) > self.scratch_size_bytes:
                self.tr.note("fb write beyond scratch")
            else:
                self.scratch[(self.fb_base // 4) + idx] = val
        elif arg == ATOM_ARG_PLL:
            idx = self.U8(ptr[0]); ptr[0] += 1
            self.card.pll_write(idx, val)
        elif arg == ATOM_ARG_MC:
            idx = self.U8(ptr[0]); ptr[0] += 1
            self.card.mc_write(idx, val)
            return

    # ==================================================================
    #  The op_* functions — each a line-for-line port. Signature mirrors
    #  atom.c: (self, ptr, arg) where ptr is the shared [offset] holder.
    # ==================================================================
    def op_add(self, ptr, arg):                                    # atom.c:602
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]
        saved = [0]
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        src = self.get_src(attr, ptr)
        dst = u32(dst + src)
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_and(self, ptr, arg):                                    # atom.c:616
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        src = self.get_src(attr, ptr)
        dst = u32(dst & src)
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_beep(self, ptr, arg):                                   # atom.c:630
        self.tr.note("ATOM BIOS beeped!")

    def op_calltable(self, ptr, arg):                              # atom.c:635
        idx = self.U8(ptr[0]); ptr[0] += 1
        self.tr.branch(f"CALLTABLE {idx}")
        r = 0
        if self.U16(self.cmd_table + 4 + 2 * idx):
            sub_ps_shift = self.ectx.ps_shift
            # atom.c: child params = ctx->ps + ctx->ps_shift (dwords). The byte
            # window accumulates across nesting.
            r = self.execute_table_locked(
                idx,
                sub_ps=self.ectx.ps,
                sub_ps_byte_off=self.ectx.ps_byte_off + sub_ps_shift * 4,
                sub_ps_size=self.ectx.ps_size - sub_ps_shift)
        else:
            self.tr.note(f"CALLTABLE {idx}: table empty, skipped")
        if r:
            self.ectx.abort = True

    def op_clear(self, ptr, arg):                                  # atom.c:651
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        attr &= 0x38
        attr |= (atom_def_dst[attr >> 3] << 6)
        self.get_dst(arg, attr, ptr, saved, 0)
        self.put_dst(arg, attr, dptr, 0, saved[0])

    def op_compare(self, ptr, arg):                                # atom.c:663
        attr = self.U8(ptr[0]); ptr[0] += 1
        dst = self.get_dst(arg, attr, ptr, None, 1)
        src = self.get_src(attr, ptr)
        self.cs_equal = (dst == src)
        self.cs_above = (dst > src)
        self.tr.branch(f"COMPARE 0x{dst:08X} vs 0x{src:08X} -> "
                       f"{'EQ' if self.cs_equal else 'NE'} "
                       f"{'GT' if self.cs_above else 'LE'}")

    def op_delay(self, ptr, arg):                                  # atom.c:677
        count = self.U8(ptr[0]); ptr[0] += 1
        self.card.delay(count, arg)

    def op_div(self, ptr, arg):                                    # atom.c:689
        attr = self.U8(ptr[0]); ptr[0] += 1
        dst = self.get_dst(arg, attr, ptr, None, 1)
        src = self.get_src(attr, ptr)
        if src != 0:
            self.divmul[0] = u32(dst // src)
            self.divmul[1] = u32(dst % src)
        else:
            self.divmul[0] = 0
            self.divmul[1] = 0

    def op_div32(self, ptr, arg):                                  # atom.c:706
        attr = self.U8(ptr[0]); ptr[0] += 1
        dst = self.get_dst(arg, attr, ptr, None, 1)
        src = self.get_src(attr, ptr)
        if src != 0:
            val64 = (dst | (self.divmul[1] << 32)) & 0xFFFFFFFFFFFFFFFF
            q = val64 // src
            self.divmul[0] = q & M32
            self.divmul[1] = (q >> 32) & M32
        else:
            self.divmul[0] = 0
            self.divmul[1] = 0

    def op_eot(self, ptr, arg):                                    # atom.c:727
        pass  # functionally a nop

    def op_jump(self, ptr, arg):                                   # atom.c:732
        target = self.U16(ptr[0])
        ptr[0] += 2
        execute = 0
        if arg == ATOM_COND_ABOVE:
            execute = self.cs_above
        elif arg == ATOM_COND_ABOVEOREQUAL:
            execute = self.cs_above or self.cs_equal
        elif arg == ATOM_COND_ALWAYS:
            execute = 1
        elif arg == ATOM_COND_BELOW:
            execute = not (self.cs_above or self.cs_equal)
        elif arg == ATOM_COND_BELOWOREQUAL:
            execute = not self.cs_above
        elif arg == ATOM_COND_EQUAL:
            execute = self.cs_equal
        elif arg == ATOM_COND_NOTEQUAL:
            execute = not self.cs_equal
        cond = {ATOM_COND_ABOVE: "ABOVE", ATOM_COND_ABOVEOREQUAL: "ABOVEOREQUAL",
                ATOM_COND_ALWAYS: "ALWAYS", ATOM_COND_BELOW: "BELOW",
                ATOM_COND_BELOWOREQUAL: "BELOWOREQUAL", ATOM_COND_EQUAL: "EQUAL",
                ATOM_COND_NOTEQUAL: "NOTEQUAL"}[arg]
        if arg != ATOM_COND_ALWAYS:
            self.tr.branch(f"JUMP.{cond} -> 0x{target:04X} taken={'yes' if execute else 'no'}")
        else:
            self.tr.branch(f"JUMP.ALWAYS -> 0x{target:04X}")
        if execute:
            if self.ectx.last_jump == (self.ectx.start + target):
                now = time.monotonic()
                if now > self.ectx.last_jump_t:
                    if (now - self.ectx.last_jump_t) > self.loop_timeout:
                        self.tr.note(f"atombios stuck in loop > {self.loop_timeout}s aborting "
                                     f"(dry-run: reg reads are 0, so a HW poll never satisfies)")
                        self.ectx.abort = True
                else:
                    self.ectx.last_jump_t = time.monotonic()
            else:
                self.ectx.last_jump = self.ectx.start + target
                self.ectx.last_jump_t = time.monotonic()
            ptr[0] = self.ectx.start + target

    def op_mask(self, ptr, arg):                                   # atom.c:786
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        mask = self.get_src_direct((attr >> 3) & 7, ptr)
        src = self.get_src(attr, ptr)
        dst = u32((dst & mask) | src)
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_move(self, ptr, arg):                                   # atom.c:803
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]
        if ((attr >> 3) & 7) != ATOM_SRC_DWORD:
            saved = [0]
            self.get_dst(arg, attr, ptr, saved, 0)
            saved_v = saved[0]
        else:
            self.skip_dst(arg, attr, ptr)
            saved_v = 0xCDCDCDCD
        src = self.get_src(attr, ptr)
        self.put_dst(arg, attr, dptr, src, saved_v)

    def op_mul(self, ptr, arg):                                    # atom.c:820
        attr = self.U8(ptr[0]); ptr[0] += 1
        dst = self.get_dst(arg, attr, ptr, None, 1)
        src = self.get_src(attr, ptr)
        self.divmul[0] = u32(dst * src)

    def op_mul32(self, ptr, arg):                                  # atom.c:831
        attr = self.U8(ptr[0]); ptr[0] += 1
        dst = self.get_dst(arg, attr, ptr, None, 1)
        src = self.get_src(attr, ptr)
        val64 = (dst * src) & 0xFFFFFFFFFFFFFFFF
        self.divmul[0] = val64 & M32
        self.divmul[1] = (val64 >> 32) & M32

    def op_nop(self, ptr, arg):                                    # atom.c:845
        pass

    def op_or(self, ptr, arg):                                     # atom.c:850
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        src = self.get_src(attr, ptr)
        dst = u32(dst | src)
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_postcard(self, ptr, arg):                               # atom.c:864
        val = self.U8(ptr[0]); ptr[0] += 1
        self.tr.note(f"POST card output: 0x{val:02X}")

    def op_repeat(self, ptr, arg):                                 # atom.c:870
        self.tr.note("repeat: unimplemented!")

    def op_restorereg(self, ptr, arg):                             # atom.c:875
        self.tr.note("restorereg: unimplemented!")

    def op_savereg(self, ptr, arg):                                # atom.c:880
        self.tr.note("savereg: unimplemented!")

    def op_setdatablock(self, ptr, arg):                           # atom.c:885
        idx = self.U8(ptr[0]); ptr[0] += 1
        if idx == 0:
            self.data_block = 0
        elif idx == 255:
            self.data_block = self.ectx.start
        else:
            self.data_block = self.U16(self.data_table + 4 + 2 * idx)
        self.tr.branch(f"SETDATABLOCK {idx} -> data_block=0x{self.data_block:04X}")

    def op_setfbbase(self, ptr, arg):                              # atom.c:899
        attr = self.U8(ptr[0]); ptr[0] += 1
        self.fb_base = self.get_src(attr, ptr)
        self.tr.branch(f"SETFBBASE -> fb_base=0x{self.fb_base:08X}")

    def op_setport(self, ptr, arg):                                # atom.c:906
        if arg == ATOM_PORT_ATI:
            port = self.U16(ptr[0])
            if not port:
                self.io_mode = ATOM_IO_MM
            else:
                self.io_mode = ATOM_IO_IIO | port
            ptr[0] += 2
            self.tr.branch(f"SETPORT ATI {port} -> io_mode="
                           f"{'MM' if self.io_mode == ATOM_IO_MM else f'IIO|{port}'}")
        elif arg == ATOM_PORT_PCI:
            self.io_mode = ATOM_IO_PCI
            ptr[0] += 1
            self.tr.branch("SETPORT PCI")
        elif arg == ATOM_PORT_SYSIO:
            self.io_mode = ATOM_IO_SYSIO
            ptr[0] += 1
            self.tr.branch("SETPORT SYSIO")

    def op_setregblock(self, ptr, arg):                            # atom.c:933
        self.reg_block = self.U16(ptr[0])
        ptr[0] += 2
        self.tr.branch(f"SETREGBLOCK -> reg_block=0x{self.reg_block:04X}")

    def op_shift_left(self, ptr, arg):                             # atom.c:940
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        attr &= 0x38
        attr |= (atom_def_dst[attr >> 3] << 6)
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        shift = self.get_src_direct(ATOM_SRC_BYTE0, ptr)
        dst = u32(dst << shift)
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_shift_right(self, ptr, arg):                            # atom.c:956
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        attr &= 0x38
        attr |= (atom_def_dst[attr >> 3] << 6)
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        shift = self.get_src_direct(ATOM_SRC_BYTE0, ptr)
        dst = (dst & M32) >> shift if shift < 32 else 0
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_shl(self, ptr, arg):                                    # atom.c:972
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        dst_align = atom_dst_to_src[(attr >> 3) & 7][(attr >> 6) & 3]
        self.get_dst(arg, attr, ptr, saved, 1)
        dst = saved[0]                     # op needs the full dst value
        shift = self.get_src(attr, ptr)
        dst = u32(dst << shift)
        dst &= atom_arg_mask[dst_align]
        dst = dst >> atom_arg_shift[dst_align]
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_shr(self, ptr, arg):                                    # atom.c:991
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        dst_align = atom_dst_to_src[(attr >> 3) & 7][(attr >> 6) & 3]
        self.get_dst(arg, attr, ptr, saved, 1)
        dst = saved[0]
        shift = self.get_src(attr, ptr)
        dst = (dst & M32) >> shift if shift < 32 else 0
        dst &= atom_arg_mask[dst_align]
        dst = dst >> atom_arg_shift[dst_align]
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_sub(self, ptr, arg):                                    # atom.c:1010
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        src = self.get_src(attr, ptr)
        dst = u32(dst - src)
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_switch(self, ptr, arg):                                 # atom.c:1024
        attr = self.U8(ptr[0]); ptr[0] += 1
        src = self.get_src(attr, ptr)
        while self.U16(ptr[0]) != ATOM_CASE_END:
            if self.U8(ptr[0]) == ATOM_CASE_MAGIC:
                ptr[0] += 1
                val = self.get_src((attr & 0x38) | ATOM_ARG_IMM, ptr)
                target = self.U16(ptr[0])
                if val == src:
                    self.tr.branch(f"SWITCH src=0x{src:X} -> case 0x{val:X} target 0x{target:04X}")
                    ptr[0] = self.ectx.start + target
                    return
                ptr[0] += 2
            else:
                self.tr.note("Bad case")
                return
        ptr[0] += 2
        self.tr.branch(f"SWITCH src=0x{src:X} -> default (fall through)")

    def op_test(self, ptr, arg):                                   # atom.c:1051
        attr = self.U8(ptr[0]); ptr[0] += 1
        dst = self.get_dst(arg, attr, ptr, None, 1)
        src = self.get_src(attr, ptr)
        self.cs_equal = ((dst & src) == 0)
        self.tr.branch(f"TEST 0x{dst:08X} & 0x{src:08X} -> {'EQ' if self.cs_equal else 'NE'}")

    def op_xor(self, ptr, arg):                                    # atom.c:1063
        attr = self.U8(ptr[0]); ptr[0] += 1
        dptr = [ptr[0]]; saved = [0]
        dst = self.get_dst(arg, attr, ptr, saved, 1)
        src = self.get_src(attr, ptr)
        dst = u32(dst ^ src)
        self.put_dst(arg, attr, dptr, dst, saved[0])

    def op_debug(self, ptr, arg):                                  # atom.c:1077
        val = self.U8(ptr[0]); ptr[0] += 1
        self.tr.note(f"DEBUG output: 0x{val:02X}")

    def op_processds(self, ptr, arg):                              # atom.c:1083
        val = self.U16(ptr[0])
        ptr[0] += val + 2
        self.tr.note(f"PROCESSDS output: 0x{val:02X}")

    # ==================================================================
    #  atom.c:1224  amdgpu_atom_execute_table_locked
    # ==================================================================
    def execute_table_locked(self, index, sub_ps=None, sub_ps_byte_off=0, sub_ps_size=None):
        base = self.U16(self.cmd_table + 4 + 2 * index)
        if not base:
            return -22  # -EINVAL

        if self.execute_depth >= ATOM_EXECUTE_MAX_DEPTH:
            self.tr.note(f"command table nesting exceeded limit ({ATOM_EXECUTE_MAX_DEPTH})")
            return -40  # -ELOOP
        self.execute_depth += 1

        length = self.U16(base + ATOM_CT_SIZE_PTR)
        ws = self.U8(base + ATOM_CT_WS_PTR)
        ps = self.U8(base + ATOM_CT_PS_PTR) & ATOM_CT_PS_MASK
        ptr = [base + ATOM_CT_CODE_PTR]

        # save/restore the enclosing exec ctx (calltable recursion)
        prev_ectx = getattr(self, "ectx", None)
        ectx = ExecContext()
        ectx.ps_shift = ps // 4
        ectx.start = base
        ectx.ps = sub_ps                # bytearray shared with parent (view via off)
        ectx.ps_byte_off = sub_ps_byte_off
        ectx.ps_size = sub_ps_size if sub_ps_size is not None else 0
        ectx.abort = False
        ectx.last_jump = 0
        ectx.last_jump_t = 0.0
        if ws:
            ectx.ws = [0] * ws
            ectx.ws_size = ws
        else:
            ectx.ws = None
            ectx.ws_size = 0
        self.ectx = ectx

        self.tr.branch(f">> execute {index}@0x{base:04X} (len {length}, WS {ws}, PS {ps})")
        self.tr.depth += 1

        ret = 0
        steps = 0
        while True:
            op = self.U8(ptr[0]); ptr[0] += 1
            steps += 1
            if steps > self.max_steps:
                self.tr.note(f"SAFETY: exceeded {self.max_steps} opcode steps — aborting")
                ret = -22
                break
            if op < len(ATOM_OP_NAMES):
                name = ATOM_OP_NAMES[op]
            else:
                name = f"[{op}]"
            self.tr.step(name, ptr[0] - 1)
            self.tr.opcode(name, ptr[0] - 1)

            if ectx.abort:
                self.tr.note(f"atombios stuck executing {index}@0x{base:04X} @ 0x{ptr[0]-1:04X}")
                ret = -22
                break

            if 0 < op < ATOM_OP_CNT:
                OPCODE_TABLE[op][0](self, ptr, OPCODE_TABLE[op][1])
            else:
                break

            if op == ATOM_OP_EOT:
                break

        self.tr.depth -= 1
        self.execute_depth -= 1
        self.ectx = prev_ectx
        return ret

    # atom.c:1302  amdgpu_atom_execute_table (resets ctx state, then locked)
    def execute_table(self, index, params):
        # reset data block / reg block / fb window / io mode / divmul
        self.data_block = 0
        self.reg_block = 0
        self.fb_base = 0
        self.io_mode = ATOM_IO_MM
        self.divmul = [0, 0]
        ps = bytearray(params)
        if len(ps) % 4 != 0:
            ps += bytes(4 - (len(ps) % 4))
        return self.execute_table_locked(index, sub_ps=ps, sub_ps_byte_off=0,
                                         sub_ps_size=len(ps) // 4)


# ---- PS accessors: ps is a bytearray, viewed at ectx.ps_byte_off, dword-indexed
def ps_dword(ectx, idx):
    o = ectx.ps_byte_off + idx * 4
    return int.from_bytes(ectx.ps[o:o + 4], "little")


def set_ps_dword(ectx, idx, val):
    o = ectx.ps_byte_off + idx * 4
    ectx.ps[o:o + 4] = (val & M32).to_bytes(4, "little")


def ctx_ps_size(atom):
    return atom.ectx.ps_size


# ============================================================================
#  opcode_table[]  (atom.c:1090) and atom_op_names[] (atom-names.h)
# ============================================================================
def _build_opcode_table():
    A = Atom  # bound below at call time via instance
    t = [None] * ATOM_OP_CNT
    spec = [
        (None, 0),
        ("op_move", ATOM_ARG_REG), ("op_move", ATOM_ARG_PS), ("op_move", ATOM_ARG_WS),
        ("op_move", ATOM_ARG_FB), ("op_move", ATOM_ARG_PLL), ("op_move", ATOM_ARG_MC),
        ("op_and", ATOM_ARG_REG), ("op_and", ATOM_ARG_PS), ("op_and", ATOM_ARG_WS),
        ("op_and", ATOM_ARG_FB), ("op_and", ATOM_ARG_PLL), ("op_and", ATOM_ARG_MC),
        ("op_or", ATOM_ARG_REG), ("op_or", ATOM_ARG_PS), ("op_or", ATOM_ARG_WS),
        ("op_or", ATOM_ARG_FB), ("op_or", ATOM_ARG_PLL), ("op_or", ATOM_ARG_MC),
        ("op_shift_left", ATOM_ARG_REG), ("op_shift_left", ATOM_ARG_PS), ("op_shift_left", ATOM_ARG_WS),
        ("op_shift_left", ATOM_ARG_FB), ("op_shift_left", ATOM_ARG_PLL), ("op_shift_left", ATOM_ARG_MC),
        ("op_shift_right", ATOM_ARG_REG), ("op_shift_right", ATOM_ARG_PS), ("op_shift_right", ATOM_ARG_WS),
        ("op_shift_right", ATOM_ARG_FB), ("op_shift_right", ATOM_ARG_PLL), ("op_shift_right", ATOM_ARG_MC),
        ("op_mul", ATOM_ARG_REG), ("op_mul", ATOM_ARG_PS), ("op_mul", ATOM_ARG_WS),
        ("op_mul", ATOM_ARG_FB), ("op_mul", ATOM_ARG_PLL), ("op_mul", ATOM_ARG_MC),
        ("op_div", ATOM_ARG_REG), ("op_div", ATOM_ARG_PS), ("op_div", ATOM_ARG_WS),
        ("op_div", ATOM_ARG_FB), ("op_div", ATOM_ARG_PLL), ("op_div", ATOM_ARG_MC),
        ("op_add", ATOM_ARG_REG), ("op_add", ATOM_ARG_PS), ("op_add", ATOM_ARG_WS),
        ("op_add", ATOM_ARG_FB), ("op_add", ATOM_ARG_PLL), ("op_add", ATOM_ARG_MC),
        ("op_sub", ATOM_ARG_REG), ("op_sub", ATOM_ARG_PS), ("op_sub", ATOM_ARG_WS),
        ("op_sub", ATOM_ARG_FB), ("op_sub", ATOM_ARG_PLL), ("op_sub", ATOM_ARG_MC),
        ("op_setport", ATOM_PORT_ATI), ("op_setport", ATOM_PORT_PCI), ("op_setport", ATOM_PORT_SYSIO),
        ("op_setregblock", 0), ("op_setfbbase", 0),
        ("op_compare", ATOM_ARG_REG), ("op_compare", ATOM_ARG_PS), ("op_compare", ATOM_ARG_WS),
        ("op_compare", ATOM_ARG_FB), ("op_compare", ATOM_ARG_PLL), ("op_compare", ATOM_ARG_MC),
        ("op_switch", 0),
        ("op_jump", ATOM_COND_ALWAYS), ("op_jump", ATOM_COND_EQUAL), ("op_jump", ATOM_COND_BELOW),
        ("op_jump", ATOM_COND_ABOVE), ("op_jump", ATOM_COND_BELOWOREQUAL),
        ("op_jump", ATOM_COND_ABOVEOREQUAL), ("op_jump", ATOM_COND_NOTEQUAL),
        ("op_test", ATOM_ARG_REG), ("op_test", ATOM_ARG_PS), ("op_test", ATOM_ARG_WS),
        ("op_test", ATOM_ARG_FB), ("op_test", ATOM_ARG_PLL), ("op_test", ATOM_ARG_MC),
        ("op_delay", ATOM_UNIT_MILLISEC), ("op_delay", ATOM_UNIT_MICROSEC),
        ("op_calltable", 0), ("op_repeat", 0),
        ("op_clear", ATOM_ARG_REG), ("op_clear", ATOM_ARG_PS), ("op_clear", ATOM_ARG_WS),
        ("op_clear", ATOM_ARG_FB), ("op_clear", ATOM_ARG_PLL), ("op_clear", ATOM_ARG_MC),
        ("op_nop", 0), ("op_eot", 0),
        ("op_mask", ATOM_ARG_REG), ("op_mask", ATOM_ARG_PS), ("op_mask", ATOM_ARG_WS),
        ("op_mask", ATOM_ARG_FB), ("op_mask", ATOM_ARG_PLL), ("op_mask", ATOM_ARG_MC),
        ("op_postcard", 0), ("op_beep", 0), ("op_savereg", 0), ("op_restorereg", 0),
        ("op_setdatablock", 0),
        ("op_xor", ATOM_ARG_REG), ("op_xor", ATOM_ARG_PS), ("op_xor", ATOM_ARG_WS),
        ("op_xor", ATOM_ARG_FB), ("op_xor", ATOM_ARG_PLL), ("op_xor", ATOM_ARG_MC),
        ("op_shl", ATOM_ARG_REG), ("op_shl", ATOM_ARG_PS), ("op_shl", ATOM_ARG_WS),
        ("op_shl", ATOM_ARG_FB), ("op_shl", ATOM_ARG_PLL), ("op_shl", ATOM_ARG_MC),
        ("op_shr", ATOM_ARG_REG), ("op_shr", ATOM_ARG_PS), ("op_shr", ATOM_ARG_WS),
        ("op_shr", ATOM_ARG_FB), ("op_shr", ATOM_ARG_PLL), ("op_shr", ATOM_ARG_MC),
        ("op_debug", 0), ("op_processds", 0),
        ("op_mul32", ATOM_ARG_PS), ("op_mul32", ATOM_ARG_WS),
        ("op_div32", ATOM_ARG_PS), ("op_div32", ATOM_ARG_WS),
    ]
    for i, (fn, a) in enumerate(spec):
        if fn is None:
            t[i] = (None, 0)
        else:
            t[i] = (getattr(A, fn), a)
    return t


# atom-names.h: atom_op_names[] (index == opcode). Debug/trace only.
ATOM_OP_NAMES = [
    "RESERVED",
    "MOVE_REG", "MOVE_PS", "MOVE_WS", "MOVE_FB", "MOVE_PLL", "MOVE_MC",
    "AND_REG", "AND_PS", "AND_WS", "AND_FB", "AND_PLL", "AND_MC",
    "OR_REG", "OR_PS", "OR_WS", "OR_FB", "OR_PLL", "OR_MC",
    "SHIFT_LEFT_REG", "SHIFT_LEFT_PS", "SHIFT_LEFT_WS", "SHIFT_LEFT_FB",
    "SHIFT_LEFT_PLL", "SHIFT_LEFT_MC",
    "SHIFT_RIGHT_REG", "SHIFT_RIGHT_PS", "SHIFT_RIGHT_WS", "SHIFT_RIGHT_FB",
    "SHIFT_RIGHT_PLL", "SHIFT_RIGHT_MC",
    "MUL_REG", "MUL_PS", "MUL_WS", "MUL_FB", "MUL_PLL", "MUL_MC",
    "DIV_REG", "DIV_PS", "DIV_WS", "DIV_FB", "DIV_PLL", "DIV_MC",
    "ADD_REG", "ADD_PS", "ADD_WS", "ADD_FB", "ADD_PLL", "ADD_MC",
    "SUB_REG", "SUB_PS", "SUB_WS", "SUB_FB", "SUB_PLL", "SUB_MC",
    "SETPORT_ATI", "SETPORT_PCI", "SETPORT_SYSIO",
    "SETREGBLOCK", "SETFBBASE",
    "COMPARE_REG", "COMPARE_PS", "COMPARE_WS", "COMPARE_FB", "COMPARE_PLL", "COMPARE_MC",
    "SWITCH",
    "JUMP", "JUMP_EQUAL", "JUMP_BELOW", "JUMP_ABOVE",
    "JUMP_BELOWOREQUAL", "JUMP_ABOVEOREQUAL", "JUMP_NOTEQUAL",
    "TEST_REG", "TEST_PS", "TEST_WS", "TEST_FB", "TEST_PLL", "TEST_MC",
    "DELAY_MILLISEC", "DELAY_MICROSEC",
    "CALLTABLE", "REPEAT",
    "CLEAR_REG", "CLEAR_PS", "CLEAR_WS", "CLEAR_FB", "CLEAR_PLL", "CLEAR_MC",
    "NOP", "EOT",
    "MASK_REG", "MASK_PS", "MASK_WS", "MASK_FB", "MASK_PLL", "MASK_MC",
    "POSTCARD", "BEEP", "SAVEREG", "RESTOREREG", "SETDATABLOCK",
    "XOR_REG", "XOR_PS", "XOR_WS", "XOR_FB", "XOR_PLL", "XOR_MC",
    "SHL_REG", "SHL_PS", "SHL_WS", "SHL_FB", "SHL_PLL", "SHL_MC",
    "SHR_REG", "SHR_PS", "SHR_WS", "SHR_FB", "SHR_PLL", "SHR_MC",
    "DEBUG", "PROCESSDS",
    "MUL32_PS", "MUL32_WS", "DIV32_PS", "DIV32_WS",
]

OPCODE_TABLE = _build_opcode_table()


# ============================================================================
#  Command param builders (atomfirmware.h struct layouts)
# ============================================================================
def build_transmitter_v1_6(phyid, action, digmode, lanenum, symclk_10khz,
                           hpdsel, digfe_sel, connobj_id):
    """struct dig_transmitter_control_ps_allocation_v1_6:
         param: dig_transmitter_control_parameters_v1_6 (16 bytes)
           phyid u8@0, action u8@1, mode_laneset(digmode) u8@2, lanenum u8@3,
           symclk_10khz u32@4, hpdsel u8@8, digfe_sel u8@9, connobj_id u8@10,
           reserved u8@11, reserved1 u32@12
         + reserved[4] u32  ->  total 32 bytes (8 dwords)
    """
    b = bytearray(32)
    b[0] = phyid & 0xFF
    b[1] = action & 0xFF
    b[2] = digmode & 0xFF
    b[3] = lanenum & 0xFF
    struct.pack_into("<I", b, 4, symclk_10khz & M32)
    b[8] = hpdsel & 0xFF
    b[9] = digfe_sel & 0xFF
    b[10] = connobj_id & 0xFF
    b[11] = 0
    struct.pack_into("<I", b, 12, 0)   # reserved1
    # bytes 16..31 = ps_allocation reserved[4] = 0
    return bytes(b)


CMD_SetPixelClock = 12   # (already defined above as a bare constant; this is its first USE)

# PIXEL_CLOCK_V7_DEEPCOLOR_RATIO_* — 0 = no deep colour (8 bpc), which is what this panel runs.
PIXEL_CLOCK_V7_DEEPCOLOR_DIS = 0


def build_set_pixel_clock_v1_7(pixclk_100hz, pll_id, encoderobjid, encoder_mode,
                               miscinfo=0, crtc_id=0,
                               deep_color_ratio=PIXEL_CLOCK_V7_DEEPCOLOR_DIS):
    """struct set_pixel_clock_parameter_v1_7 (16 bytes):
         pixclk_100hz     u32@0   target pixel clock, 100 Hz units
         pll_id           u8 @4   WHICH PLL -- see the refusal in main(); this is #12's phyid
         encoderobjid     u8 @5
         encoder_mode     u8 @6   ATOM_ENCODER_MODE_*
         miscinfo         u8 @7   PIXEL_CLOCK_V7_MISC_*
         crtc_id          u8 @8
         deep_color_ratio u8 @9
         reserved1[2]     u8 @10
         reserved2        u32@12
       Padded to an 8-dword ps_allocation, matching how #76 is invoked (ps_size=8).

    ⚠ REVISION DERIVED FROM THE ROM, NOT ASSUMED: the MasterCommandTable entry for index 12 on
    archaemenid's VBIOS reports rev 1.7 (table @0x98EE, 1052 bytes, WS 16 B). If a different card
    reports another revision this struct is WRONG and must not be sent -- check before trusting it.
    """
    b = bytearray(32)
    struct.pack_into("<I", b, 0, pixclk_100hz & M32)
    b[4] = pll_id & 0xFF
    b[5] = encoderobjid & 0xFF
    b[6] = encoder_mode & 0xFF
    b[7] = miscinfo & 0xFF
    b[8] = crtc_id & 0xFF
    b[9] = deep_color_ratio & 0xFF
    b[10] = 0
    b[11] = 0
    struct.pack_into("<I", b, 12, 0)
    return bytes(b)


def build_encoder_stream_setup_v1_5(digid, digmode, lanenum, pclk_10khz,
                                    bitpercolor=0, dplinkrate=0):
    """struct dig_encoder_stream_setup_parameters_v1_5 (12 bytes):
         digid u8@0, action u8@1(=STREAM_SETUP), digmode u8@2, lanenum u8@3,
         pclk_10khz u32@4, bitpercolor u8@8, dplinkrate_270mhz u8@9, reserved[2]
       Padded to the encoder ps_allocation size (union, keep 12+pad = 16 -> 4 dw)
    """
    b = bytearray(16)
    b[0] = digid & 0xFF
    b[1] = ATOM_ENCODER_CMD_STREAM_SETUP
    b[2] = digmode & 0xFF
    b[3] = lanenum & 0xFF
    struct.pack_into("<I", b, 4, pclk_10khz & M32)
    b[8] = bitpercolor & 0xFF
    b[9] = dplinkrate & 0xFF
    return bytes(b)


def build_encoder_generic_v1_5(digid, action):
    """struct dig_encoder_generic_cmd_parameters_v1_5 (12 bytes):
         digid u8@0, action u8@1, reserved1[2], reserved2[2] -> pad to 16."""
    b = bytearray(16)
    b[0] = digid & 0xFF
    b[1] = action & 0xFF
    return bytes(b)


# ============================================================================
#  phyid / digfe_sel / connobj_id derivation from the object-info data table
# ============================================================================
#  phyid from a REAL register snapshot — the only trustworthy derivation
# ----------------------------------------------------------------------------
# DIG_BE_CNTL is BASE_IDX-2 relative 0x20AF with a 0x100 per-instance stride; absolute = +0x34C0, so
# instance N lives at 0x556F + N*0x100. DIG_BE_EN_CNTL is one dword higher (0x20B0 -> 0x5570 + N*0x100).
_DIG_BE_CNTL_ABS0 = 0x556F
_DIG_BE_EN_ABS0 = 0x5570
_DIG_STRIDE = 0x100
_DIG_COUNT = 5                      # Renoir: 5 stream encoders


def derive_phyid_from_snapshot(snap):
    """Which link-encoder instance is actually driving? Mirrors kernel gpu_phy_discover().

    Three conditions, all required — any one alone can read stale on a re-routed pipe, and a wrong answer
    aims ATOM #76 (a PHY power-cycle) at the wrong transmitter:
      1. DIG_BE_EN_CNTL bit0 (DIG_ENABLE) set  — the back end is live
      2. DIG_BE_CNTL FE_SOURCE_SELECT [15:8] non-zero — a front end is routed to it
      3. DIG_BE_CNTL DIG_MODE [18:16] in {2 DVI, 3 HDMI} — a real signalling mode (1 = off)

    Returns (phyid, note) or (None, None) when no snapshot / nothing live. NEVER falls back to 0: "0" is
    the wrong-and-plausible answer a dead PHY accepts in silence.
    """
    if not snap:
        return None, None
    for i in range(_DIG_COUNT):
        be = snap.get(_DIG_BE_CNTL_ABS0 + i * _DIG_STRIDE)
        en = snap.get(_DIG_BE_EN_ABS0 + i * _DIG_STRIDE)
        if be is None or en is None:
            continue
        mode = (be >> 16) & 0x7
        fesel = (be >> 8) & 0xFF
        if (en & 1) and fesel and mode in (2, 3):
            sig = "DVI" if mode == 2 else "HDMI"
            return i, (f"phyid={i} DERIVED FROM SNAPSHOT: DIG_BE_CNTL[{i}]=0x{be:08X} "
                       f"(mode={mode} {sig}, FE_SOURCE=0x{fesel:02X}), DIG_BE_EN_CNTL[{i}]=0x{en:08X} "
                       f"(DIG_ENABLE set)")
    return None, None


def derive_transmitter_routing(atom):
    """Walk display_object_info_table_v1_4 and find the HDMI connector's path.
    Returns a dict with the raw objids + a best-effort phyid/digfe_sel/connobj,
    and a list of human-readable notes documenting the derivation.

    NOTE (TODO for iron): the exact phyid the amdgpu DC layer passes is
    link->link_enc->transmitter - TRANSMITTER_UNIPHY_A, assigned from the DDC/HPD
    routing at runtime — NOT trivially the encoder ENUM_ID. We surface the object
    table evidence and pick a documented default; the operator should confirm
    phyid against amdgpu's own atomfirmware trace on archaemenid before --live.
    """
    notes = []
    dt = atom.data_table
    obj = atom.U16(dt + 4 + 2 * 22)   # data table index 22 = Object_Header
    rev = (atom.U8(obj + 2), atom.U8(obj + 3))
    notes.append(f"Object_Header @0x{obj:04X} rev {rev[0]}.{rev[1]}")
    npath = atom.U8(obj + 6)
    p = obj + 8
    hdmi = None
    paths = []
    CONNECTOR_HDMI_TYPE_A = 0x13
    for k in range(npath):
        if p + 18 > len(atom.bios):
            break
        disp = atom.U16(p); enc = atom.U16(p + 4); dtag = atom.U16(p + 12)
        conn_type = (disp >> 12) & 0x7
        conn_id = disp & 0xFF
        enc_type = (enc >> 12) & 0x7
        enc_enum = (enc >> 8) & 0x7
        enc_id = enc & 0xFF
        paths.append((disp, enc, dtag, conn_id, enc_enum, enc_id))
        # connector object (type 3), HDMI_TYPE_A id
        if conn_type == 3 and conn_id == CONNECTOR_HDMI_TYPE_A and hdmi is None:
            hdmi = (disp, enc, dtag, conn_id, enc_enum, enc_id)
        p += 18
    for (disp, enc, dtag, cid, ee, eid) in paths:
        notes.append(f"  path conn=0x{disp:04X}(id0x{cid:02X}) enc=0x{enc:04X}"
                     f"(enum{ee} id0x{eid:02X}) devtag=0x{dtag:04X}")
    if hdmi is None:
        notes.append("  no HDMI_TYPE_A connector path found — using amdgpu defaults")
        return {"phyid": 0, "digfe_sel": 0x02, "connobj_id": 0x13,
                "found": False, "notes": notes}
    disp, enc, dtag, cid, ee, eid = hdmi
    notes.append(f"  HDMI path: connector 0x{disp:04X}, internal encoder enum_id={ee}")
    # Best-effort: UNIPHY encoder ENUM_ID is 1-based; phyid (UNIPHYA=0) ~ enum-1.
    phyid_guess = max(ee - 1, 0)
    notes.append(f"  phyid (best-effort = enc_enum-1) = {phyid_guess}  [TODO confirm on iron]")
    notes.append("  digfe_sel = 0x02 (DIG1 front-end, bit1) per 'live encoder is DIG1'")
    notes.append(f"  connobj_id = 0x{cid:02X} (HDMI_TYPE_A)")
    return {"phyid": phyid_guess, "digfe_sel": 0x02, "connobj_id": cid,
            "found": True, "notes": notes}


# ============================================================================
#  Runner
# ============================================================================
def load_snapshot(path):
    """Optional --regsnapshot: lines of 'IDX VALUE' (hex ok) -> {idx: val}."""
    snap = {}
    with open(path) as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if not line:
                continue
            a, b = line.split()
            snap[int(a, 0)] = int(b, 0) & M32
    return snap


def run_command(atom, tracer, index, params, title):
    print("=" * 78)
    print(f" run_command: {title}")
    print(f"   table index {index}  params ({len(params)} B = {len(params)//4} dwords): "
          f"{params.hex()}")
    print("=" * 78)
    tracer.hist.clear()
    tracer.events.clear()
    tracer.n_reg_r = tracer.n_reg_w = tracer.n_delay = 0
    ret = atom.execute_table(index, params)
    print("-" * 78)
    status = "reached EOT / clean return" if ret == 0 else f"returned {ret}"
    print(f" << table {index} finished: {status}")
    print(f"    REG reads={tracer.n_reg_r}  REG writes={tracer.n_reg_w}  delays={tracer.n_delay}")
    print(" opcode histogram (this table):")
    for name in sorted(tracer.hist, key=lambda n: (-tracer.hist[n], n)):
        print(f"    {tracer.hist[name]:4d}  {name}")
    print(f"    total opcodes stepped: {sum(tracer.hist.values())}")
    return ret


def main():
    ap = argparse.ArgumentParser(description="Faithful atom.c interpreter (dry-run default)")
    ap.add_argument("--rom", default=os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                  "..", "vbios.rom"))
    ap.add_argument("--live", action="store_true",
                    help="OPERATOR ONLY: mmap BAR5 and do REAL MMIO (default is dry-run)")
    ap.add_argument("--resource5",
                    default="/sys/bus/pci/devices/0000:04:00.0/resource5")
    ap.add_argument("--regsnapshot", help="dry-run: file of 'IDX VALUE' seed reads")
    ap.add_argument("--command", default="transmitter",
                    choices=["transmitter", "encoder-setup", "encoder-enable", "pixelclock"])
    ap.add_argument("--action", default="enable",
                    help="transmitter action: enable|disable|init|enable_output|setup")
    ap.add_argument("--phyid", type=lambda s: int(s, 0), default=None,
                    help="override derived phyid")
    ap.add_argument("--digmode", default="hdmi", choices=["hdmi", "dvi", "dp"])
    ap.add_argument("--lanenum", type=int, default=4)
    ap.add_argument("--symclk", type=int, default=24150, help="symclk in 10kHz (241.50MHz)")
    ap.add_argument("--pixclk", type=int, default=2415020,
                    help="pixelclock: target pixel clock in 100Hz units "
                         "(default 2415020 = 241.502 MHz, the measured inherited clock)")
    ap.add_argument("--pll-id", type=lambda s: int(s, 0), default=None,
                    help="pixelclock: WHICH PLL. No default on purpose -- see the refusal below")
    ap.add_argument("--crtc-id", type=int, default=0, help="pixelclock: OTG/pipe index")
    ap.add_argument("--miscinfo", type=lambda s: int(s, 0), default=0,
                    help="pixelclock: PIXEL_CLOCK_V7_MISC_* bits")
    ap.add_argument("--loop-timeout", type=float, default=2.0,
                    help="dry-run: seconds before a HW-poll loop is declared stuck")
    ap.add_argument("--histogram-only", action="store_true",
                    help="step both tables, print histograms, no per-op trace")
    ap.add_argument("--quiet", action="store_true", help="suppress the per-op trace")
    args = ap.parse_args()

    rom_path = os.path.abspath(args.rom)
    bios = open(rom_path, "rb").read()

    tracer = Tracer(verbose=not (args.quiet or args.histogram_only))
    snap = load_snapshot(args.regsnapshot) if args.regsnapshot else None
    if args.live:
        print("!!! --live: REAL BAR5 MMIO. This will drive the transmitter. !!!")
        card = LiveCard(tracer, args.resource5)
    else:
        card = DryRunCard(tracer, snap)

    atom = Atom(bios, card, tracer, loop_timeout=args.loop_timeout)
    print(f"ROM parsed from {rom_path}")
    print(f"  ATOM_ROM_HEADER @0x{atom.rom:04X}  MasterCommandTable @0x{atom.cmd_table:04X}  "
          f"MasterDataTable @0x{atom.data_table:04X}")
    print(f"  IIO programs indexed: {sum(1 for x in atom.iio if x)} "
          f"(empty IndirectIOAccess => tables use direct MMIO)")

    # ---- derive routing from the object-info table ----
    route = derive_transmitter_routing(atom)
    print("\nTransmitter routing derivation (object-info data table):")
    for n in route["notes"]:
        print("  " + n)
    # ---- PREFER a snapshot-derived phyid over the object-table guess -------------------------------
    # The object-table value is a GUESS (enc ENUM_ID - 1) and it was WRONG on archaemenid: it said 0, the
    # live transmitter is 1, and #76 at phyid=0 would have power-cycled a dead PHY. See the M8d tracker.
    # When a --regsnapshot of real hardware is supplied we can do far better than guess — ask the back end
    # the same way the kernel's gpu_phy_discover() does: the live link encoder is the DIG_BE_CNTL instance
    # that is ENABLED, has a front end routed, and carries a real signalling mode (2=DVI, 3=HDMI; 1=off).
    snap_phyid, snap_note = derive_phyid_from_snapshot(snap)
    if snap_phyid is not None:
        print(f"  {snap_note}")
    phyid = args.phyid if args.phyid is not None else (
        snap_phyid if snap_phyid is not None else route["phyid"])
    src = ("--phyid override" if args.phyid is not None else
           "snapshot-derived" if snap_phyid is not None else
           "object-table GUESS -- UNRELIABLE, supply --regsnapshot or --phyid")
    digfe_sel = route["digfe_sel"]
    connobj = route["connobj_id"]
    print(f"  => using phyid={phyid}, digfe_sel=0x{digfe_sel:02X}, connobj_id=0x{connobj:02X}"
          f"  ({src})\n")

    digmode = {"hdmi": ATOM_ENCODER_MODE_HDMI, "dvi": ATOM_ENCODER_MODE_DVI,
               "dp": ATOM_ENCODER_MODE_DP}[args.digmode]
    action = {"enable": ATOM_TRANSMITTER_ACTION_ENABLE,
              "disable": ATOM_TRANSMITTER_ACTION_DISABLE,
              "init": ATOM_TRANSMITTER_ACTION_INIT,
              "enable_output": ATOM_TRANSMITTER_ACTION_ENABLE_OUTPUT,
              "setup": ATOM_TRANSMITTER_ACTION_SETUP}[args.action]

    if args.histogram_only:
        tracer.verbose = False
        # DIGxEncoderControl STREAM_SETUP(HDMI) and DIG1TransmitterControl(ENABLE)
        enc = build_encoder_stream_setup_v1_5(ATOM_ENCODER_CONFIG_V5_DIG1_ENCODER,
                                              ATOM_ENCODER_MODE_HDMI, args.lanenum, args.symclk)
        run_command(atom, tracer, CMD_DIGxEncoderControl, enc,
                    "DIGxEncoderControl(STREAM_SETUP, HDMI) [histogram]")
        atom2 = Atom(bios, card, tracer, loop_timeout=args.loop_timeout)
        atom2.ectx = None
        tx = build_transmitter_v1_6(phyid, action, digmode, args.lanenum, args.symclk,
                                    hpdsel=0, digfe_sel=digfe_sel, connobj_id=connobj)
        run_command(atom2, tracer, CMD_DIG1TransmitterControl, tx,
                    "DIG1TransmitterControl(ENABLE, HDMI) [histogram]")
        return

    if args.command == "transmitter":
        tx = build_transmitter_v1_6(phyid, action, digmode, args.lanenum, args.symclk,
                                    hpdsel=0, digfe_sel=digfe_sel, connobj_id=connobj)
        run_command(atom, tracer, CMD_DIG1TransmitterControl, tx,
                    f"DIG1TransmitterControl(action={args.action}, {args.digmode}, "
                    f"phyid={phyid}, lanenum={args.lanenum}, symclk={args.symclk})")
    elif args.command == "encoder-setup":
        enc = build_encoder_stream_setup_v1_5(ATOM_ENCODER_CONFIG_V5_DIG1_ENCODER,
                                              digmode, args.lanenum, args.symclk)
        run_command(atom, tracer, CMD_DIGxEncoderControl, enc,
                    f"DIGxEncoderControl(STREAM_SETUP, {args.digmode}, DIG1)")
    elif args.command == "pixelclock":
        # ⛔⛔ REFUSE RATHER THAN GUESS -- THIS IS #12's phyid AND THE LESSON IS SIX DAYS OLD.
        # MD-2 recorded phyid=0 "SETTLED, object-info-derived" and it was the DEAD instance; firing
        # #76 at it produced an 87,292-read poll storm against unconfigured silicon. The kernel's fix
        # was NOT to flip the constant but to make atom_run_transmitter_enable_hdmi() REFUSE when it
        # cannot derive the target. pll_id is the same shape of parameter for the same command class,
        # so it gets the same treatment here: no default, no fallback, no "probably 0".
        if args.pll_id is None:
            print("\nREFUSED: --pll-id is required for --command pixelclock.")
            print("  pll_id selects WHICH PLL #12 programs. It is this command's phyid, and phyid")
            print("  stood wrong at 0 for six days while pointing at a dead instance. There is no")
            print("  safe default: guessing sends a clock command at a PLL that may be driving the")
            print("  live pipe. Derive it from a --regsnapshot of real silicon, or pass it")
            print("  explicitly and record where the value came from.")
            return 2
        # ⛔⛔ THE 20-24 BAND DRIVES A PHY, NOT A PLL. Measured 2026-07-30 by sweeping the WHOLE
        # parameter space against a real iron snapshot, which the original 3..8 sweep did not reach:
        #     0-2      bail, 9 opcodes, zero activity (reads exactly like success)
        #     3-19     benign — 55 opcodes, writes 0x0140/0x0141 (DCCG), reads 0x5001 (OTG0_CONTROL)
        #     20-24    ⛔ 175k-510k opcodes, up to 177k reads (a poll storm, the wrong-phyid #76
        #              signature), each value writing a DIFFERENT PHY/RDPCS instance block on a 0xD8
        #              stride, and CALLING ATOM TABLE 77
        #     25-254   bail, 7 opcodes        255  ATOM_PPLL_INVALID — benign, same DCCG footprint
        # This tool exists to CHARACTERISE those, so it warns rather than refusing — but it must say
        # so loudly, because "#12's blast radius is disjoint from #76's" was written into this repo
        # on the strength of the 3..8 slice and is false for exactly this band.
        if 20 <= args.pll_id <= 24:
            print(f"\n⛔ WARNING: pll_id={args.pll_id} is in the PHY band (20-24), not the PLL band.")
            print("   On real silicon this writes a PHY/RDPCS instance block and calls table 77 —")
            print("   a SUPERSET of what #76 writes, and #76 has blanked this panel twice.")
            print("   The kernel's atom_run_set_pixel_clock() REFUSES this band. Dry-run only.\n")
        rev = atom.U8(atom.U16(atom.cmd_table + 4 + CMD_SetPixelClock * 2) + 3)
        if rev != 7:
            print(f"\nREFUSED: this ROM's SetPixelClock table is rev 1.{rev}, not 1.7.")
            print("  build_set_pixel_clock_v1_7 would pack the WRONG struct. Transcribe the")
            print(f"  matching set_pixel_clock_parameter_v1_{rev} before running this.")
            return 2
        pc = build_set_pixel_clock_v1_7(args.pixclk, args.pll_id,
                                        encoderobjid=connobj, encoder_mode=digmode,
                                        miscinfo=args.miscinfo, crtc_id=args.crtc_id)
        run_command(atom, tracer, CMD_SetPixelClock, pc,
                    f"SetPixelClock(v1.7, pixclk={args.pixclk} x100Hz, pll_id={args.pll_id}, "
                    f"crtc={args.crtc_id}, {args.digmode})")
        # ⭐ THE ATTESTATION: which parameter dwords did the bytecode actually read?
        rd = sorted(atom.ps_read)
        print(f"\nPS attestation -- dwords the table READ: {rd}")
        print(f"  bytes covered: {[f'{i*4}..{i*4+3}' for i in rd]}")
        if not rd:
            print("  ⚠ NONE. The table consumed no parameters on this path -- either it bailed")
            print("    early (read the trace) or the struct is not reaching it. Do not conclude")
            print("    the layout is right from a clean run that read nothing.")
        return 0

    elif args.command == "encoder-enable":
        enc = build_encoder_generic_v1_5(ATOM_ENCODER_CONFIG_V5_DIG1_ENCODER,
                                         ATOM_ENCODER_CMD_ENABLE_DIG)
        run_command(atom, tracer, CMD_DIGxEncoderControl, enc,
                    "DIGxEncoderControl(ENABLE_DIG, DIG1)")


if __name__ == "__main__":
    sys.exit(main() or 0)
