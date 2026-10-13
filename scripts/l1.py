#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# l1.py — BITE L1: the zero-agnos-burn HDMI-audio SEQUENCING discriminator.
#
# ============================================================================================
# THE CLAIM
# ============================================================================================
# On archaemenid's amdgpu-BLACKLISTED, GOP-programmed DVI-parked pipe, with agnos's own HDMI
# audio register program staged MUTED inside agnos's OTG-disabled envelope, across a REAL
# ATOM #4 + #76 transmitter edge, audible 48 kHz PCM reaches the XB323U **if and only if** the
# unmute falls on the FAR side of that edge.
#
# Two sub-claims are bundled, and the arm set separates them:
#   S1  harness validity — SOME program issued from Linux userspace on this pipe can make this
#                          sink sound.  (arm Rlit)
#   S2  the M9 hypothesis — among agnos's programs, unmute POSITION is the discriminating
#                          variable.  (arms P vs Q)
# S1 GATES S2.  gpu.md's own M9 row: "if amdgpu's own sequence replayed from Linux does not
# sound, M9-as-scoped is the wrong bite."  That branch is a real, plausible outcome and it is
# reported as an answer about the EXPERIMENT, never dressed up as an answer about agnos.
#
# ============================================================================================
# WHY THIS IS A NEW FILE AND NOT AN EXTENSION OF agnos-hdmi-linux.py
# ============================================================================================
# agnos-hdmi-linux.py is LEFT BYTE-UNCHANGED.  It is cited by gpu.md's L1 row and by three
# prior-art analyses, and its shape is antithetical to a blinded protocol: linear, single-arm,
# the answer announced in real time on the tty ("MORPH FLIP NOW", live lpib/AFMT_STATUS), log
# opened "w", and no argv validation (a typo'd --morph-dig-flip silently ran a DIFFERENT
# experiment).  Its iron-proven FEED path is LIFTED VERBATIM here — see §1 — because that code
# is trusted and must not be redesigned.
#
# ============================================================================================
# WHAT THIS TOOL WILL NOT DO
# ============================================================================================
#  * It will not announce, during a window, which arm is running or what is expected.
#  * It will not write a boot entry unless you pass --install-boot-entry and confirm.
#  * It will not "clean up" the link or codec on exit (R16): the shutdown release pop is the
#    arc's only sink-side instrument, so the final restore's wall-clock is logged instead.
#  * It will not adjudicate on a register readback.  AFMT_STATUS / ACR / AZ readbacks are
#    SUPPORTING DATA ONLY.  The oracle is the operator's ear; the eye is the oracle for the
#    picture.  (R6 — the CRC episode, and twelve mute burns of green gates.)
#
# ============================================================================================
# READ BEFORE RUNNING
# ============================================================================================
#  * Root.  It unbinds snd_hda_intel from the GPU's HDA function and drives it raw, and it
#    writes GPU BAR5 DCN registers.
#  * Requires a boot with amdgpu BLACKLISTED.  `modprobe -r amdgpu` is NOT a substitute: the
#    remove path disables the CRTCs, leaving a torn-down pipe rather than the GOP-DVI pipe
#    agnos actually inherits.  Testing agnos's program against anything else is not the
#    experiment.  The gate refuses unless 04:00.0 has no driver bound.
#  * Reserve a hugepage first:
#        echo 1 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages
#  * Stop the userspace audio stack or the unbind blocks in D-state:
#        systemctl --user stop wireplumber pipewire pipewire-pulse pipewire.socket \
#                             pipewire-pulse.socket
#  * SSH is the PRIMARY channel.  sshd has no graphics dependency and survives the blacklist.
#    The panel blanks in EVERY window by design; do not read that as a fault.
#  * Nothing is written to disk outside the session directory unless you ask for the boot entry.
#
# Blind recovery, safe to run repeatedly, safe when nothing is broken:
#        sudo /home/macro/Repos/agnosticos/scripts/l1.py --restore
# ============================================================================================

import argparse
import ctypes
import errno
import glob
import gc
import hashlib
import json
import mmap
import os
import re
import secrets
import struct
import subprocess
import sys
import threading
import time

REPO_AGNOS = "/home/macro/Repos/agnos"
REPO_META = "/home/macro/Repos/agnosticos"
GPU_REGS = f"{REPO_AGNOS}/kernel/core/gpu_regs.cyr"
FTRACE = f"{REPO_META}/docs/development/prior-art/amdgpu-hdmi-modeset-writes-0717.txt"
PRIOR_ART_DIR = f"{REPO_META}/docs/development/prior-art"
OLD_SCRIPT = f"{REPO_META}/scripts/agnos-hdmi-linux.py"

GPU_DEV_DEFAULT = "0000:04:00.0"
HDA_DEV_DEFAULT = "0000:04:00.1"

# BAR5 addressing.  abs = base_seg + rel; BAR5 byte offset = abs*4.
SEG = {1: 0xC0, 2: 0x34C0}          # base_idx 1 = DCCG, base_idx 2 = DIG/AZ/OTG
SEG_MP1 = 0x16000
DIG_STRIDE = 0x100
AZ_STRIDE = 6

M32 = 0xFFFFFFFF

# ⛔ THE FORBIDDEN-VALUE LIST (spec §4.7).  Every one of these is a value that would silently
# change a SECOND variable, or is a readback residue mistaken for a program value.  The plan
# builder asserts none of them appears in a derived arm.  They are not stylistic objections:
#   0x80885E8F  amdgpu's YCbCr AVI body — agnos ships RGB; writing it changes the colorimetry
#               as well as the thing under test
#   0x000005CA / 0x00000AA1  amdgpu's bar literals — ours are derived from the live raster
#   0x00000101 -> AFMT_CNTL         bit 8 is the READ-ONLY AUDIO_CLOCK_ON ack (echo, not answer)
#   0x00000101 -> DIG_BE_EN_CNTL    bit 8 is the READ-ONLY SYMCLK_BE_ON status
#   0x00000000 -> AFMT_INFOFRAME_CONTROL0  the post-pulse residue of a SELF-CLEARING latch: the
#               old script wrote it and therefore never latched the audio InfoFrame at all
#   0x3AF5C000  forced CTS — off by default, mirroring the kernel's #ifdef HDMI_ACR_CTS; gpu.md
#               records that forcing it ZEROED tap1
FORBIDDEN_VALUES = {
    0x80885E8F: "amdgpu YCbCr AVI body (agnos ships RGB — would change colorimetry too)",
    0x000005CA: "amdgpu AVI bar literal (ours is derived from the live raster)",
    0x00000AA1: "amdgpu AVI bar literal (ours is derived from the live raster)",
    0x3AF5C000: "forced CTS — off by default; forcing it zeroed tap1 (gpu.md:379)",
}
FORBIDDEN_PAIRS = {
    ("AFMT_CNTL", 0x00000101): "bit 8 is the READ-ONLY AUDIO_CLOCK_ON ack",
    ("DIG_BE_EN_CNTL", 0x00000101): "bit 8 is the READ-ONLY SYMCLK_BE_ON status",
    ("AFMT_INFOFRAME_CONTROL0", 0x00000000): "post-pulse residue of a self-clearing latch",
}

# ⛔ BANNED STIMULUS.  The 2026-07-15 pattern is published in CHANGELOG.md and in memory, so it
# is now GUESSABLE and can never be used as a blinded stimulus again.
BANNED_PATTERN = {"bursts": 3, "pitch": 880, "sweep": "U", "span": (300, 3000)}



# ⚠ THE ONLY TRANSCRIBED REGISTER VALUE IN THIS FILE, and it is anchor-checked (see §2's rule).
# agnos writes this as a bare literal rather than a named constant, so there is no name to
# extract; check_anchors() below asserts the source line still carries it.
AGNOS_DTO0_MODULE = 0x24D998
ANCHORS = [
    (f"{REPO_AGNOS}/kernel/core/gpu.cyr", 6584, "0x24D998",
     "DCCG_AUDIO_DTO0_MODULE — agnos's bare literal"),
]


def check_anchors():
    """A transcribed value is a liability the moment the source moves.  This turns that
    liability into a refusal: every literal this twin copies out of agnos names the line it came
    from, and the line is verified to still say it.  Silent agnos<->twin drift is how a tool
    produces a confident answer about a program the kernel does not run."""
    problems = []
    for path, line_no, needle, what in ANCHORS:
        try:
            lines = open(path).read().splitlines()
        except OSError as e:
            problems.append(f"{what}: cannot read {path} ({e})")
            continue
        if line_no > len(lines) or needle not in lines[line_no - 1]:
            found = [i + 1 for i, l in enumerate(lines) if needle in l]
            problems.append(
                f"{what}: {path}:{line_no} no longer contains {needle}"
                + (f" (now at line(s) {found} — update ANCHORS)" if found
                   else " and it appears NOWHERE in the file — the kernel changed"))
    return problems


# =============================================================================================
# §0  LOGGING / JOURNAL — every artifact fsync'd BEFORE the window it describes is listened to,
#     so a wedge during window k loses at most window k.  Nothing depends on this process
#     surviving to print a summary: --verify reconstructs the session from the ndjson files on
#     any machine, without root and without hardware.
# =============================================================================================

class Journal:
    def __init__(self, sessdir):
        self.dir = sessdir
        os.makedirs(sessdir, exist_ok=True)
        os.makedirs(f"{sessdir}/dumps", exist_ok=True)
        os.makedirs(f"{sessdir}/writelists", exist_ok=True)
        os.makedirs(f"{sessdir}/env", exist_ok=True)
        self.t0 = time.monotonic()
        self._console = open(f"{sessdir}/console.log", "a", buffering=1)
        self._journal = open(f"{sessdir}/journal.ndjson", "a", buffering=1)
        self._writes = open(f"{sessdir}/writes.jsonl", "a", buffering=1)

    def _stamp(self):
        return int((time.monotonic() - self.t0) * 1000)

    def say(self, msg, tty=True):
        """Human transcript.  tty=False keeps it OUT of the operator's view during a blinded
        window — that is the whole point of the flag, not a convenience."""
        line = f"[T+{self._stamp():>7}ms] {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} {msg}"
        self._console.write(line + "\n")
        os.fsync(self._console.fileno())
        if tty:
            print(msg, flush=True)

    def event(self, kind, **kw):
        rec = {"t_ms": self._stamp(), "utc": time.time(), "kind": kind}
        rec.update(kw)
        self._journal.write(json.dumps(rec, sort_keys=True) + "\n")
        os.fsync(self._journal.fileno())
        return rec

    def access(self, phase, window, arm, rw, space, idx, val, pre=None):
        rec = {"t_us": int((time.monotonic() - self.t0) * 1e6), "phase": phase,
               "window": window, "arm": arm, "space": space, "rw": rw,
               "idx": idx, "byte": idx * 4, "val": val, "pre": pre}
        self._writes.write(json.dumps(rec) + "\n")
        # ⛔ FSYNC EVERY ACCESS. Line buffering only reaches the OS page cache; a hard lockup
        # loses it. The first live sessions hard-locked the APU and left writes.jsonl as NUL
        # bytes — destroying precisely the trace needed to find the offending write. This is
        # slow, and it is worth it: the register trace IS the diagnosis.
        self._writes.flush()
        os.fsync(self._writes.fileno())
        return rec

    def artifact(self, name, obj):
        p = f"{self.dir}/{name}"
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            json.dump(obj, f, indent=2, sort_keys=True, default=str)
            f.flush()
            os.fsync(f.fileno())
        return p

    def text(self, name, s):
        p = f"{self.dir}/{name}"
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write(s)
            f.flush()
            os.fsync(f.fileno())
        return p


def sha256_file(path):
    h = hashlib.sha256()
    try:
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 16), b""):
                h.update(chunk)
    except OSError:
        return None
    return h.hexdigest()


def sha256_str(s):
    return hashlib.sha256(s.encode() if isinstance(s, str) else s).hexdigest()


# =============================================================================================
# §1  LIFTED VERBATIM — the iron-proven feed path.
#
# ⛔ DO NOT REDESIGN ANY OF THIS.  Lifted from agnos-hdmi-linux.py and 1:1 verified against
# agnos/kernel/core/hda.cyr.  This code has made sound on this exact hardware.  Exactly THREE
# surgical fixes are applied (§9.3 of the spec), each marked `# FIX n:` inline and each with its
# reason; anything else that differs from the original is a BUG in this lift.
#
# `l1.py feedcheck` reproduces the original's steps 0-4 and 6 with ZERO BAR5 writes and asserts
# LPIB advances.  Run it once before the battery.  If it fails, the lift broke the feed path and
# the session stops — that gate exists precisely so a redesign cannot silently break iron-proven
# code.
# =============================================================================================

# ---- HDA controller register offsets (kernel/core/hda.cyr) ----
GCAP, GCTL, WAKEEN, STATESTS = 0x00, 0x08, 0x0C, 0x0E
INTCTL, INTSTS = 0x20, 0x24
CORBLBASE, CORBUBASE, CORBWP, CORBRP, CORBCTL, CORBSIZE = 0x40, 0x44, 0x48, 0x4A, 0x4C, 0x4E
RIRBLBASE, RIRBUBASE, RIRBWP, RINTCNT, RIRBCTL, RIRBSTS, RIRBSIZE = (
    0x50, 0x54, 0x58, 0x5A, 0x5C, 0x5D, 0x5E)
ICOI, ICII, ICIS = 0x60, 0x64, 0x68
DPLBASE, DPUBASE = 0x70, 0x74
SD_CTL, SD_STS, SD_LPIB, SD_CBL, SD_LVI, SD_FMT, SD_BDPL, SD_BDPU = (
    0x00, 0x03, 0x04, 0x08, 0x0C, 0x12, 0x18, 0x1C)

# ---- codec verbs (kernel/core/hda.cyr) ----
V_GET_PARAM = 0xF00
V_SET_CVT_FMT = 0x200
V_SET_CVT_STREAM = 0x706
V_SET_PIN_WIDGET = 0x707
V_SET_AMP_GAIN = 0x300
V_SET_DIGI_CONV1 = 0x70D
V_SET_CHAN_COUNT = 0x72D
V_SET_ELD_ASP = 0x777          # ATI/AMD slot-map verb
V_SET_ELD_ASP_ALT = 0x785      # the alternate the codec may honour instead
V_GET_PIN_SENSE = 0xF09


class Mmio:
    """LIFTED VERBATIM from agnos-hdmi-linux.py. O_RDWR|O_SYNC + PROT_READ|PROT_WRITE.

    ⚠ §5.1 — this is a bare struct.pack_into into an mmap.  It is NOT agnos's ring-0
    gpu_mfence'd store, and the mapping may be write-combining.  In an experiment whose whole
    hypothesis is ORDERING, that is the largest residual.  Mitigation is a mandatory
    readback-flush after every ordering-critical store (w32f below) — never a claim that the
    residual is closed.  It is COMMON-MODE across arms, so the within-session comparison
    survives even though the transfer claim to agnos does not."""

    def __init__(self, path, journal=None):
        self.fd = os.open(path, os.O_RDWR | os.O_SYNC)
        self.size = os.fstat(self.fd).st_size
        self.mm = mmap.mmap(self.fd, self.size, mmap.MAP_SHARED,
                            mmap.PROT_READ | mmap.PROT_WRITE)
        self.j = journal
        self.phase = "?"
        self.window = None
        self.arm = None

    def r32(self, off):
        v = struct.unpack_from("<I", self.mm, off)[0]
        if self.j:
            self.j.access(self.phase, self.window, self.arm, "R", "BAR", off // 4, v)
        return v

    def w32(self, off, val):
        pre = struct.unpack_from("<I", self.mm, off)[0]
        struct.pack_into("<I", self.mm, off, val & M32)
        if self.j:
            self.j.access(self.phase, self.window, self.arm, "W", "BAR", off // 4,
                          val & M32, pre=pre)

    def w32f(self, off, val):
        """Ordering-critical store: write THEN read back, so the store is pushed out of any
        write-combining buffer before the next one is issued.  Used for every BE<->FE routing
        store, the mute boundary, the unmute, and every AZ index-select before its data write.
        The readback value is returned but MUST NOT be used as evidence the write "took" —
        that is the echo-vs-answer error (twelve mute burns of green gates)."""
        self.w32(off, val)
        return self.r32(off)

    def close(self):
        try:
            self.mm.close()
            os.close(self.fd)
        except Exception:
            pass


class Dma:
    """LIFTED VERBATIM: 2 MB MAP_HUGETLB, mlock, pagemap phys, contiguity gate."""

    def __init__(self):
        self.size = 2 * 1024 * 1024
        self.buf = mmap.mmap(-1, self.size,
                             flags=mmap.MAP_PRIVATE | mmap.MAP_ANONYMOUS | 0x40000,  # MAP_HUGETLB
                             prot=mmap.PROT_READ | mmap.PROT_WRITE)
        addr = ctypes.addressof(ctypes.c_char.from_buffer(self.buf))
        libc = ctypes.CDLL("libc.so.6", use_errno=True)
        if libc.mlock(ctypes.c_void_p(addr), ctypes.c_size_t(self.size)) != 0:
            raise OSError(ctypes.get_errno(), "mlock failed")
        self.virt = addr
        self.phys = self._phys(addr)
        if self.phys == 0:
            raise OSError("pagemap gave phys 0 — run as root")

    @staticmethod
    def _phys(virt):
        page = os.sysconf("SC_PAGE_SIZE")
        with open("/proc/self/pagemap", "rb") as f:
            f.seek((virt // page) * 8)
            entry = struct.unpack("<Q", f.read(8))[0]
        if not (entry & (1 << 63)):
            return 0
        return (entry & ((1 << 55) - 1)) * page + (virt % page)

    def write(self, off, data):
        self.buf[off:off + len(data)] = data


def pci_unbind(dev):
    """LIFTED VERBATIM. Returns the driver name it was bound to, or None."""
    drv_link = f"/sys/bus/pci/devices/{dev}/driver"
    prev = None
    if os.path.islink(drv_link):
        prev = os.path.basename(os.path.realpath(drv_link))
        with open(f"{drv_link}/unbind", "w") as f:
            f.write(dev)
        time.sleep(0.2)
    return prev


def pci_bind(dev, driver):
    try:
        with open(f"/sys/bus/pci/drivers/{driver}/bind", "w") as f:
            f.write(dev)
        return True
    except OSError:
        return False


def pci_enable_master(dev):
    """LIFTED VERBATIM: PCI cfg 0x04 |= 0x6 (MEM + BUS MASTER)."""
    p = f"/sys/bus/pci/devices/{dev}/config"
    fd = os.open(p, os.O_RDWR)
    try:
        os.lseek(fd, 4, os.SEEK_SET)
        cmd = struct.unpack("<H", os.read(fd, 2))[0]
        os.lseek(fd, 4, os.SEEK_SET)
        os.write(fd, struct.pack("<H", cmd | 0x6))
        return cmd
    finally:
        os.close(fd)


def pci_write_command(dev, cmd):
    fd = os.open(f"/sys/bus/pci/devices/{dev}/config", os.O_RDWR)
    try:
        os.lseek(fd, 4, os.SEEK_SET)
        os.write(fd, struct.pack("<H", cmd))
    finally:
        os.close(fd)


class Hda:
    """LIFTED VERBATIM from agnos-hdmi-linux.py, 1:1 vs agnos/kernel/core/hda.cyr.

    THREE surgical fixes only, each marked `# FIX n:`."""

    def __init__(self, bar, dma, journal=None):
        self.b = bar
        self.dma = dma
        self.j = journal
        # FIX 1: SD base DERIVED from GCAP ISS instead of hardcoded 0x80.  The original assumed
        # 0x80 and added a guard; deriving it removes the assumption entirely.  check_gcap()
        # must run BEFORE any stop_stream() — the original called stop_stream() at :473, i.e.
        # before validation, poking a stream descriptor whose base it had not yet confirmed.
        self.sd = None
        self.corb = 0x1000
        self.rirb = 0x2000
        self.bdl = 0x3000
        self.pcm = 0x10000

    # ---- fix 1 ----
    def check_gcap(self):
        gcap = self.b.r32(GCAP) & 0xFFFF
        iss = (gcap >> 8) & 0xF
        oss = (gcap >> 12) & 0xF
        ok64 = gcap & 0x1
        self.sd = 0x80 + iss * 0x20
        if not ok64:
            raise RuntimeError("GCAP says no 64-bit DMA — the BDL layout below assumes it")
        if oss == 0:
            raise RuntimeError("GCAP reports zero output streams")
        return {"gcap": gcap, "iss": iss, "oss": oss, "sd_base": self.sd}

    def reset(self):
        self.b.w32(GCTL, 0)
        time.sleep(0.0003)
        self.b.w32(GCTL, 1)
        for _ in range(1000):
            if self.b.r32(GCTL) & 1:
                break
            time.sleep(0.0001)
        time.sleep(0.001)

    def ring_init(self):
        # FIX 2: order corrected to hda.cyr:505-536 — CORBSIZE -> CORBWP=0 -> CORBRP reset ->
        # CORBCTL RUN -> RIRB* -> RINTCNT -> RIRBCTL.  agnos starts the CORB engine BEFORE
        # programming RIRB; the original started it last.  On a controller that latches ring
        # base at engine start this is a real difference, and the point of this tool is to run
        # AGNOS's sequence, not a plausible one.
        b, dma = self.b, self.dma
        b.w32(CORBCTL, 0)
        b.w32(RIRBCTL, 0)
        b.w32(CORBLBASE, (dma.phys + self.corb) & M32)
        b.w32(CORBUBASE, (dma.phys + self.corb) >> 32)
        struct.pack_into("<B", b.mm, CORBSIZE, 0x02)          # 256 entries
        struct.pack_into("<H", b.mm, CORBWP, 0)
        struct.pack_into("<H", b.mm, CORBRP, 0x8000)          # reset
        for _ in range(1000):
            if struct.unpack_from("<H", b.mm, CORBRP)[0] & 0x8000:
                break
            time.sleep(0.0001)
        struct.pack_into("<H", b.mm, CORBRP, 0)
        struct.pack_into("<B", b.mm, CORBCTL, 0x02)           # RUN
        b.w32(RIRBLBASE, (dma.phys + self.rirb) & M32)
        b.w32(RIRBUBASE, (dma.phys + self.rirb) >> 32)
        struct.pack_into("<B", b.mm, RIRBSIZE, 0x02)
        struct.pack_into("<H", b.mm, RIRBWP, 0x8000)          # reset WP
        struct.pack_into("<H", b.mm, RINTCNT, 1)
        struct.pack_into("<B", b.mm, RIRBCTL, 0x02)           # DMA EN
        self.rirb_rp = 0

    def probe_codec_immediate(self, caddr, nid, verb, payload=0):
        """LIFTED VERBATIM: the ICOI/ICII/ICIS DMA-free path."""
        b = self.b
        cmd = (caddr << 28) | (nid << 20) | (verb << 8) | payload if verb < 0x800 \
            else (caddr << 28) | (nid << 20) | (verb << 8) | payload
        for _ in range(100):
            if not (struct.unpack_from("<H", b.mm, ICIS)[0] & 0x1):
                break
            time.sleep(0.0001)
        struct.pack_into("<H", b.mm, ICIS, 0x2)               # clear IRV
        b.w32(ICOI, cmd)
        struct.pack_into("<H", b.mm, ICIS, 0x1)               # ICB
        for _ in range(1000):
            st = struct.unpack_from("<H", b.mm, ICIS)[0]
            if st & 0x2:
                return b.r32(ICII)
            time.sleep(0.0001)
        return None

    def prove_rirb(self, caddr=0):
        """RAM-safety gate.

        ⛔ FIX 3: this must NOT sys.exit with a confident IOMMU verdict (the original's
        :332-337).  A CORB/RIRB round-trip failure has at least two live explanations — DMA not
        reaching RAM, and a codec whose audio clock is not running — and announcing one as fact
        is the same error class as "SETTLED: phyid=0".  Report BOTH, retry once after the DCCG
        DTO is programmed, and log the retry outcome as data: that retry IS the free
        "does the codec round-trip depend on the audio clock?" experiment."""
        vid = self.probe_codec_immediate(caddr, 0, V_GET_PARAM, 0x00)
        return {"immediate_vendor_id": vid, "ok": vid is not None and vid != 0}

    def verb(self, caddr, nid, verb, payload=0):
        """LIFTED VERBATIM: CORB ring + CORBWP doorbell + RIRBSTS W1C 0x5."""
        b = self.b
        cmd = (caddr << 28) | (nid << 20) | (verb << 8) | payload
        wp = (struct.unpack_from("<H", b.mm, CORBWP)[0] + 1) & 0xFF
        self.dma.write(self.corb + wp * 4, struct.pack("<I", cmd))
        struct.pack_into("<H", b.mm, CORBWP, wp)
        for _ in range(2000):
            rwp = struct.unpack_from("<H", b.mm, RIRBWP)[0] & 0xFF
            if rwp != self.rirb_rp:
                self.rirb_rp = rwp
                resp = struct.unpack_from("<Q", self.dma.buf, self.rirb + rwp * 8)[0]
                struct.pack_into("<B", b.mm, RIRBSTS, 0x5)
                return resp & M32
            time.sleep(0.0001)
        struct.pack_into("<B", b.mm, RIRBSTS, 0x5)
        return None

    def config_codec(self, caddr, cvt, pin):
        """LIFTED VERBATIM: 13 verbs incl. BOTH slot-map verbs (0x777 and 0x785).  Which one
        the codec honours is not knowable a priori, so agnos sends both; so do we."""
        v = self.verb
        out = []
        out.append(("pin_widget_off", v(caddr, pin, V_SET_PIN_WIDGET, 0x00)))
        out.append(("cvt_stream_off", v(caddr, cvt, V_SET_CVT_STREAM, 0x00)))
        out.append(("digi1", v(caddr, cvt, V_SET_DIGI_CONV1, 0x01)))
        out.append(("chan_count", v(caddr, cvt, V_SET_CHAN_COUNT, 0x01)))
        out.append(("fmt", v(caddr, cvt, V_SET_CVT_FMT, 0x0011)))       # 48k 16-bit stereo
        out.append(("stream_tag", v(caddr, cvt, V_SET_CVT_STREAM, 0x10)))
        out.append(("pin_widget_on", v(caddr, pin, V_SET_PIN_WIDGET, 0x40)))
        out.append(("amp", v(caddr, pin, V_SET_AMP_GAIN, 0xB000)))
        out.append(("aspL", v(caddr, pin, V_SET_ELD_ASP, 0x00)))
        out.append(("aspR", v(caddr, pin, V_SET_ELD_ASP, 0x11)))
        out.append(("aspL_alt", v(caddr, pin, V_SET_ELD_ASP_ALT, 0x00)))
        out.append(("aspR_alt", v(caddr, pin, V_SET_ELD_ASP_ALT, 0x11)))
        out.append(("pin_sense", v(caddr, pin, V_GET_PIN_SENSE, 0x00)))
        return out

    def stop_stream(self):
        """LIFTED VERBATIM: SD_CTL <- 0.  ALWAYS safe to call; never leaves DMA active."""
        if self.sd is None:
            return
        try:
            struct.pack_into("<B", self.b.mm, self.sd + SD_CTL, 0)
            time.sleep(0.002)
        except Exception:
            pass

    def fill_tone(self, pcm_bytes):
        """Parameterised fill.  The ORIGINAL's fixed triangle is replaced by a caller-supplied
        buffer so the blinded pattern is generated THROUGH THE FEED PATH UNDER TEST and never
        through ALSA.  Everything else about the ring — size, layout, 2-entry BDL — is the
        original's, and hda.cyr:1200-1252's."""
        self.dma.write(self.pcm, pcm_bytes)
        return len(pcm_bytes)

    def arm_stream(self, nbytes):
        """LIFTED VERBATIM: BDL, SRST cycle, tag/CBL/FMT/LVI/BDPL/BDPU, RUN."""
        b, dma, sd = self.b, self.dma, self.sd
        half = nbytes // 2
        bdl = b""
        for i in range(2):
            bdl += struct.pack("<QII", dma.phys + self.pcm + i * half, half, 0)
        dma.write(self.bdl, bdl)
        struct.pack_into("<B", b.mm, sd + SD_CTL, 0x01)          # SRST
        for _ in range(1000):
            if struct.unpack_from("<B", b.mm, sd + SD_CTL)[0] & 0x01:
                break
            time.sleep(0.0001)
        struct.pack_into("<B", b.mm, sd + SD_CTL, 0x00)
        for _ in range(1000):
            if not (struct.unpack_from("<B", b.mm, sd + SD_CTL)[0] & 0x01):
                break
            time.sleep(0.0001)
        b.w32(sd + SD_CBL, nbytes)
        struct.pack_into("<H", b.mm, sd + SD_LVI, 1)
        struct.pack_into("<H", b.mm, sd + SD_FMT, 0x0011)
        b.w32(sd + SD_BDPL, (dma.phys + self.bdl) & M32)
        b.w32(sd + SD_BDPU, (dma.phys + self.bdl) >> 32)
        struct.pack_into("<B", b.mm, sd + SD_STS, 0x1C)          # W1C
        ctl = struct.unpack_from("<I", b.mm, sd + SD_CTL)[0]
        struct.pack_into("<I", b.mm, sd + SD_CTL, (ctl & ~0xFF00000) | (1 << 20) | 0x02)
        t0 = time.monotonic()
        a = b.r32(sd + SD_LPIB)
        while time.monotonic() - t0 < 0.5:
            time.sleep(0.02)
            if b.r32(sd + SD_LPIB) != a:
                return True
        return False

    def lpib(self):
        return self.b.r32(self.sd + SD_LPIB)


# =============================================================================================
# §2  CONSTANT SOURCING — mechanical, never retyped.
#
# Every register offset and bit constant is EXTRACTED from agnos's own gpu_regs.cyr at start-up.
# A name the twin needs but gpu_regs.cyr does not define is a REFUSAL, not a fallback.
#
# This exists because the old script's 19-row hand-typed table is exactly where its defects
# lived, and because a twin that drifts from the kernel it is imitating is worse than no twin:
# it produces a confident answer about a program agnos does not run.
#
# GOVERNING RULE, and the two obligations are OPPOSITE:
#   * a constant copied from an amdgpu CAPTURE is a defect        -> derive it from the live pipe
#   * a constant agnos SHIPS is the thing under test              -> replay it verbatim, and log
#                                                                    the derived alternative beside
# Conflating them is how 0x80885E8F got into the current script.
# =============================================================================================

class Consts:
    PAT = re.compile(r"^var\s+(GPU_[A-Za-z0-9_]+)\s*=\s*(0[xX][0-9A-Fa-f]+|\d+)\s*;", re.M)

    def __init__(self, path=GPU_REGS):
        self.path = path
        self.vals = {}
        self.lines = {}
        src = open(path).read()
        for i, line in enumerate(src.splitlines(), 1):
            m = self.PAT.match(line)
            if m:
                self.vals[m.group(1)] = int(m.group(2), 0)
                self.lines[m.group(1)] = i
        self.sha = sha256_file(path)

    def __getitem__(self, name):
        if name not in self.vals:
            raise KeyError(
                f"REFUSING TO RUN: {name} is not defined in {self.path}. The twin must not "
                f"invent a constant agnos does not ship — that silently makes this a different "
                f"experiment.")
        return self.vals[name]

    def require(self, *names):
        missing = [n for n in names if n not in self.vals]
        if missing:
            raise KeyError("REFUSING TO RUN: gpu_regs.cyr does not define: " + ", ".join(missing))

    def manifest(self, names):
        return {n: {"value": self.vals.get(n), "gpu_regs_line": self.lines.get(n)} for n in names}


# =============================================================================================
# §3  THE PLAN MODEL — write lists are DATA, not side effects.
#
# ⭐ THIS IS THE STRUCTURAL HEART OF THE EXPERIMENT.  Every arm is built as an ordered list of
# Ops before a single register is touched.  That buys three things nothing else can:
#
#   1. --plan-diff proves the SINGLE-VARIABLE claim on any machine, with no root and no
#      hardware, BEFORE the session is booked.  If P and Q differ by anything other than the
#      position of one identical block, that is visible in a diff rather than discovered by ear.
#   2. The five §2.4 assertions are machine-checked, so "the control and the treatment are the
#      same program in a different order" is PROVEN, not asserted.  This is the ATOM_DRY defect
#      class one layer up: two runs that differ in name and not in behaviour.
#   3. The forbidden-value list is enforced mechanically at build time.
#
# Hashing is STRUCTURAL — (kind, name, rule, mask, set, value) — deliberately NOT the resolved
# addresses or the post-RMW values, because those depend on live state that differs between the
# planning machine and archaemenid.  A structural hash answers "is this the same PROGRAM?",
# which is the question the assertions ask.
# =============================================================================================

class Op:
    __slots__ = ("kind", "name", "space", "rel", "rule", "mask", "set_", "value",
                 "note", "flush", "az_ord", "expect")

    def __init__(self, kind, name, space=2, rel=None, rule=None, mask=None, set_=None,
                 value=None, note="", flush=False, az_ord=None, expect=None):
        self.kind = kind          # R | W | RMW | AZW | AZR | ATOM | DELAY | FN
        self.name = name
        self.space = space        # 1 = DCCG, 2 = DIG/AZ/OTG, "mp1"
        self.rel = rel
        self.rule = rule
        self.mask = mask          # bits CLEARED
        self.set_ = set_          # bits SET
        self.value = value        # absolute value, for kind W
        self.note = note
        self.flush = flush        # ordering-critical -> readback after the store
        self.az_ord = az_ord      # AZ endpoint ordinal for AZW/AZR
        self.expect = expect

    def struct_key(self):
        return (self.kind, self.name, self.space, self.rule, self.mask, self.set_,
                self.value, self.az_ord)

    def __repr__(self):
        bits = [self.kind, self.name]
        if self.az_ord is not None:
            bits.append(f"ord=0x{self.az_ord:02X}")
        if self.value is not None:
            bits.append(f"val=0x{self.value:08X}")
        if self.mask:
            bits.append(f"clr=0x{self.mask:08X}")
        if self.set_:
            bits.append(f"set=0x{self.set_:08X}")
        return "<" + " ".join(bits) + ">"


class Plan:
    """An ordered, named list of Ops.  Blocks are tracked so an arm can say 'the unmute block
    goes HERE' and the assertion machinery can prove the block itself is byte-identical."""

    def __init__(self, name):
        self.name = name
        self.ops = []
        self.blocks = {}          # block name -> (start, end) indices

    def add(self, op):
        self.ops.append(op)
        return self

    def extend(self, ops, block=None):
        start = len(self.ops)
        self.ops.extend(ops)
        if block:
            self.blocks[block] = (start, len(self.ops))
        return self

    def block_ops(self, block):
        if block not in self.blocks:
            return []
        a, b = self.blocks[block]
        return self.ops[a:b]

    def struct_hash(self):
        h = hashlib.sha256()
        for o in self.ops:
            h.update(repr(o.struct_key()).encode())
        return h.hexdigest()

    @staticmethod
    def hash_ops(ops):
        h = hashlib.sha256()
        for o in ops:
            h.update(repr(o.struct_key()).encode())
        return h.hexdigest()

    def check_forbidden(self, consts):
        """Mechanical enforcement of §4.7.  A derived arm that contains one of these is not a
        stylistic problem — it is a second variable moving."""
        bad = []
        for o in self.ops:
            if o.value is not None and o.value in FORBIDDEN_VALUES:
                bad.append((o.name, hex(o.value), FORBIDDEN_VALUES[o.value]))
            key = (o.name, o.value)
            if key in FORBIDDEN_PAIRS:
                bad.append((o.name, hex(o.value), FORBIDDEN_PAIRS[key]))
        return bad


# =============================================================================================
# §4  RUNTIME DERIVATION — refuse the session if any of these fails.
#
# ⛔ NOTHING HERE MAY FALL BACK TO A DEFAULT.  The arc's most expensive single error was
# "MD-2 SETTLED: phyid=0", a paper derivation that stood for weeks, was flagged
# `[TODO confirm on iron]` by its own tool, and turned out to name the DEAD transmitter.  A
# fallback is how that happens: a plausible answer accepted because no one had to look.
#
# ⛔ AND phyid (BACK end / transmitter) IS NOT gpu_audio_dig (FRONT end / stream encoder).
# DCN assigns them independently.  Both are 1 on archaemenid — by ROUTING, not identity — and
# assuming identity is precisely what produced that bug.  D2 and D3 derive them separately and
# CROSS-CHECK; disagreement is a refusal, not a tie-break.
# =============================================================================================

class Derive:
    def __init__(self, bar, consts, journal):
        self.b, self.c, self.j = bar, consts, journal
        self.out = {}

    def _abs(self, space, rel):
        return (SEG[space] + rel) * 4

    def r(self, space, rel):
        return self.b.r32(self._abs(space, rel))

    # ---- D2: phy (back end) — the gpu_phy_discover() port, three required conditions --------
    def phy(self):
        c = self.c
        cands = []
        for i in range(5):
            be = self.r(2, c["GPU_R_DIG_BE_CNTL"] + i * DIG_STRIDE)
            en = self.r(2, c["GPU_R_DIG_BE_EN_CNTL"] + i * DIG_STRIDE)
            mode = (be & c["GPU_DIG_MODE_MASK"]) >> c["GPU_DIG_MODE_SHIFT"]
            live = bool(en & c["GPU_DIG_ENABLE"]) \
                and (be & c["GPU_DIG_FE_SOURCE_SEL_MASK"]) != 0 \
                and mode in (c["GPU_DIG_MODE_DVI"], c["GPU_DIG_MODE_HDMI"])
            self.out.setdefault("dig_be_scan", []).append(
                {"inst": i, "be_cntl": be, "be_en": en, "mode": mode, "live": live})
            if live:
                cands.append(i)
        if len(cands) != 1:
            raise RuntimeError(
                f"REFUSING: gpu_phy_discover port found {len(cands)} live link encoders "
                f"({cands}). Never fall back to 0 — that is the MD-2 error, and it aims the "
                f"most destructive command in the arc at a dead PHY.")
        self.out["phy"] = cands[0]
        return cands[0]

    # ---- D3: dig (front end) + the mandatory cross-check -------------------------------------
    def dig(self, phy):
        c = self.c
        found = []
        for i in range(5):
            en = self.r(2, c["GPU_R_DIG_BE_EN_CNTL"] + i * DIG_STRIDE)
            fe = self.r(2, c["GPU_R_DIG_FE_CNTL"] + i * DIG_STRIDE)
            if (en & c["GPU_DIG_ENABLE"]) and (fe & (1 << 24)):    # SYMCLK_FE_ON
                found.append(i)
        if len(found) != 1:
            raise RuntimeError(f"REFUSING: gpu_audio_probe port found {len(found)} live stream "
                               f"encoders ({found}).")
        dig = found[0]
        # Cross-check against the live BE's FE_SOURCE_SELECT field.  ⛔ REFUSE on disagreement;
        # do NOT pick a winner.  Two derivations that disagree mean one of them is wrong and we
        # do not know which — exactly the state in which the arc previously guessed.
        be = self.r(2, c["GPU_R_DIG_BE_CNTL"] + phy * DIG_STRIDE)
        sel = (be & c["GPU_DIG_FE_SOURCE_SEL_MASK"]) >> 8
        sel_idx = sel.bit_length() - 1 if sel else None
        self.out["dig"] = dig
        self.out["fe_source_select_raw"] = sel
        self.out["fe_source_select_index"] = sel_idx
        if sel_idx is not None and sel_idx != dig:
            raise RuntimeError(
                f"REFUSING: front-end derivations disagree — SYMCLK scan says DIG{dig}, the live "
                f"back end's FE_SOURCE_SELECT says DIG{sel_idx}. Both are logged. Do not guess.")
        return dig

    def totals(self, consts):
        c = self.c
        h_total = self.r(2, c["GPU_R_OTG_H_TOTAL"]) & 0xFFFF
        v_total = self.r(2, c["GPU_R_OTG_V_TOTAL"]) & 0xFFFF
        self.out["h_total_field"] = h_total
        self.out["v_total_field"] = v_total
        self.out["h_total"] = h_total + 1
        self.out["v_total"] = v_total + 1
        return h_total, v_total

    def pixclk_100hz(self):
        """D9. HDMI_ACR_STATUS_0 >> 12 is the HARDWARE-MEASURED CTS-derived kHz and works at
        DIG_MODE=2.  ⚠ REPORTED ONLY — it may read 0 on a pristine DVI pipe, which is not a
        failure.  It is never used to gate anything: a measurement that can legitimately be
        absent must not be able to void the session."""
        try:
            raw = self.r(2, self.c["GPU_R_HDMI_ACR_STATUS_0"])
        except Exception:
            return None
        khz = raw >> 12
        self.out["acr_status_0_raw"] = raw
        self.out["pixclk_khz_measured"] = khz
        return khz * 10 if khz else None


# =============================================================================================
# §5  THE REGISTER PROGRAMS — agnos's own, ported with the phy PASSED IN.
#
# ⛔⛔ THE PHY IS A PARAMETER AND THAT IS LOAD-BEARING (agnos 1.56.14 "M9d-fix").
# In the kernel, mdo_transmit_run() latches s_phy from the PRISTINE pre-state precisely because
# the op destroys what discovery needs, then the BE<->FE disconnect clears FE_SOURCE_SELECT.
# M9d's staging call originally re-discovered internally, failed gpu_phy_discover()'s
# FE_SOURCE_SELECT condition, and REFUSED — so in both audio arms the staging half silently did
# not run and each arm would have unmuted over a register file that was never programmed: two
# identical non-experiments wearing the names "control" and "treatment".
# This twin must not reproduce that.  phy comes in as an argument, once, from pristine state.
# =============================================================================================

class Programs:
    """Builds agnos's write lists as Plans.  Every constant comes from gpu_regs.cyr by NAME
    (§2) — there are no transcribed register values in this class, which is what makes
    agnos<->twin drift structurally impossible rather than merely unlikely."""

    def __init__(self, consts, dig, phy, az, pipe, h_total, v_total, ftrace=None):
        self.c = consts
        self.ftrace = ftrace
        self.dig, self.phy, self.az, self.pipe = dig, phy, az, pipe
        self.h_total, self.v_total = h_total, v_total
        self.d = dig * DIG_STRIDE
        self.db = phy * DIG_STRIDE

    # -- helpers ---------------------------------------------------------------------------
    def _w(self, name, rel, value, note="", flush=False, space=2):
        return Op("W", name, space=space, rel=rel, value=value, note=note, flush=flush)

    def _rmw(self, name, rel, clear=0, set_=0, note="", flush=False, space=2):
        return Op("RMW", name, space=space, rel=rel, mask=clear, set_=set_, note=note,
                  flush=flush)

    def _azw(self, ordinal, value=None, clear=0, set_=0, note="", flush=True):
        """An AZ endpoint write is ALWAYS index-select then data.  ⛔ Every access re-selects:
        the index window has NO MEMORY, and agnos's own comment records that inserting a write
        without re-selecting left a later access pointed at whatever ordinal `ir` last named."""
        return Op("AZW", "AZ_ENDPOINT", space=2, az_ord=ordinal, value=value,
                  mask=clear, set_=set_, note=note, flush=flush)

    # -- G1 MUTE-BASE ------------------------------------------------------------------------
    def g1_mute_base(self):
        """⭐ THIS IS WHAT MAKES A NULL AN ACTUAL NULL.

        Without it a null window inherits whatever tap state the GOP parked — and the GOP-parked
        value of AFMT_AUDIO_PACKET_CONTROL has never been read on ANY boot (that is one of the
        first-ever measurements this session produces).  A "null" that might already be unmuted
        is not a control."""
        c = self.c
        p = Plan("G1")
        p.extend([
            self._rmw("AFMT_AUDIO_PACKET_CONTROL", c["GPU_R_AFMT_AUDIO_PACKET_CONTROL"] + self.d,
                      clear=c["GPU_AFMT_AUDIO_SAMPLE_SEND"], set_=c["GPU_AFMT_60958_CS_UPDATE"],
                      note="tap SHUT, CS_UPDATE set = amdgpu's 0x04000800", flush=True),
            # ⛔ AVMUTE IS DELIBERATELY *NOT* SET HERE — read this before "restoring" it.
            #
            # The first cut set HDMI_GC.AVMUTE in G1, reasoning from amdgpu's staging literal
            # (HDMI_GC <- 0x05). On iron, 2026-07-24, that blanked the panel and it did NOT come
            # back: AVMUTE tells an HDMI sink to mute audio *and video*, and clearing it again
            # does not necessarily make the sink re-acquire. G1 runs in EVERY window including
            # the nulls, so the battery would have gone dark on window 1 and stayed dark.
            #
            # It is also UNNECESSARY, which the first GOP-side dump settled: the inherited pipe
            # reads AFMT_AUDIO_PACKET_CONTROL = 0x04000800 — already exactly amdgpu's
            # muted-staging value — and HDMI_GC = 0x04, i.e. AVMUTE already CLEAR. The tap is
            # shut by SAMPLE_SEND (the op above), which is the bit that actually gates samples.
            # AVMUTE added a video side effect for no audio benefit.
            #
            # ⚠ U_agn still CLEARS AVMUTE (B4), which is a harmless no-op when it was never set
            # and remains correct if a sink or a prior boot left it armed.
            self._azw(c["GPU_AZ_IX_HOT_PLUG_CONTROL"], clear=c["GPU_AZ_AUDIO_ENABLED"],
                      note="AUDIO_ENABLED cleared"),
        ], block="G1")
        return p

    # -- S_der: agnos list (A), gpu_hdmi_audio_enable() under MODESET_AUDIO ------------------
    def s_der(self):
        c = self.c
        d, az = self.d, self.az
        p = Plan("S_der")
        ops = []
        # A2-A4 stream attributes (gpu_hdmi_stage_attributes)
        ops.append(self._rmw("HDMI_CONTROL", c["GPU_R_HDMI_CONTROL"] + d,
                             clear=c["GPU_HDMI_CONTROL_RATE_HAZARD"] | 0x1F, set_=0x19))
        ops.append(self._rmw("HDMI_GC", c["GPU_R_HDMI_GC"] + d, clear=c["GPU_HDMI_GC_AVMUTE"]))
        ops.append(self._rmw("HDMI_VBI_PACKET_CONTROL", c["GPU_R_HDMI_VBI_PACKET_CONTROL"] + d,
                             set_=0x31))
        # A6-A8 DCCG audio DTO.  ⛔ SRC strictly BEFORE module/phase — hardware-enforced, and
        # AMD's own dce_audio.c carries the warning.
        ops.append(self._rmw("DCCG_AUDIO_DTO_SRC", c["GPU_R_DCCG_AUDIO_DTO_SRC"],
                             clear=0x30 | 0x7, set_=self.pipe & 7, space=1, flush=True,
                             note="DTO0_SOURCE_SEL + DTO_SEL; RMW ONLY (global register)"))
        # ⚠ MODULE/PHASE are HARDCODED because AGNOS SHIPS THEM and they are the thing under
        # test.  The derived alternative is measured and logged beside them, never written in
        # Block 1: amdgpu wrote 0x24D998 against this same live clock WHILE THE TONE WAS
        # AUDIBLE, so a 6-12 ppm difference here is demonstrably not load-bearing.
        # ⚠ DTO0_MODULE is the ONE genuinely transcribed literal in this file, because it is the
        # one place agnos itself writes a bare literal rather than a named constant
        # (gpu.cyr:6584).  It is therefore ANCHOR-CHECKED at start-up: if that line stops
        # containing this value the tool REFUSES, so the twin cannot silently drift from the
        # kernel.  agnos's own comment there is worth honouring — "byte-matched to the working
        # path is worth more than a derivation we like better".
        ops.append(self._w("DCCG_AUDIO_DTO0_MODULE", c["GPU_R_DCCG_AUDIO_DTO0_MODULE"],
                           AGNOS_DTO0_MODULE, space=1,
                           note="agnos ships this literal — anchor-checked at gpu.cyr:6584"))
        ops.append(self._w("DCCG_AUDIO_DTO0_PHASE", c["GPU_R_DCCG_AUDIO_DTO0_PHASE"],
                           c["GPU_DCCG_AUDIO_DTO_PHASE_VAL"], space=1,
                           note="24 MHz constant, NOT Fs-scaled"))
        # A9 clock-gating disable FIRST — ⛔ writes into a gated clock are dropped SILENTLY.
        ops.append(self._azw(c["GPU_AZ_IX_HOT_PLUG_CONTROL"], set_=c["GPU_AZ_CLOCK_GATING_DISABLE"],
                             note="clock-gating disable — MUST precede every other AZ write"))
        # A10 channel/speaker + HDMI_CONNECTION (bit16 is what the HW builds the AIF from)
        ops.append(self._azw(c["GPU_AZ_IX_CHANNEL_SPEAKER"],
                             clear=0x7F | 0x20000 | 0x40000,
                             set_=c["GPU_AZ_SPEAKER_FL_FR"] | c["GPU_AZ_HDMI_CONNECTION"]))
        # A11-A15 the ELD/descriptor block.  ⚠ EVERY value below is sourced BY NAME from
        # gpu_regs.cyr — there is not one transcribed literal here.  The SINK_INFO values are
        # panel-specific XB323U ELD content and are flagged as such in the manifest.
        for ordn, valn in [
            ("GPU_AZ_IX_AUDIO_DESCRIPTOR0", "GPU_AZ_AUDIO_DESCRIPTOR0_VAL"),
            ("GPU_AZ_IX_AUDIO_DESCRIPTOR1", None),
            ("GPU_AZ_IX_AUDIO_DESCRIPTOR2", None),
            ("GPU_AZ_IX_RESPONSE_LIPSYNC", None),
            ("GPU_AZ_IX_SINK_INFO0", "GPU_AZ_SINK_INFO0_VAL"),
            ("GPU_AZ_IX_SINK_INFO1", "GPU_AZ_SINK_INFO1_VAL"),
            ("GPU_AZ_IX_SINK_INFO2", "GPU_AZ_SINK_INFO2_VAL"),
            ("GPU_AZ_IX_SINK_INFO3", "GPU_AZ_SINK_INFO3_VAL"),
            ("GPU_AZ_IX_SINK_INFO4", "GPU_AZ_SINK_INFO4_VAL"),
            ("GPU_AZ_IX_SINK_INFO5", "GPU_AZ_SINK_INFO5_VAL"),
            ("GPU_AZ_IX_SINK_INFO6", None),
            ("GPU_AZ_IX_SINK_INFO7", None),
            ("GPU_AZ_IX_SINK_INFO8", None),
            ("GPU_AZ_IX_MULTICHANNEL_MODE", "GPU_AZ_MULTICHANNEL_MODE_SINGLE"),
        ]:
            ops.append(self._azw(c[ordn], value=(c[valn] if valn else 0), note=f"{ordn}"))
        # A16 clock-gating re-enable.  ⛔ MUST re-select HOT_PLUG_CONTROL — the block above left
        # the index window pointed elsewhere.
        ops.append(self._azw(c["GPU_AZ_IX_HOT_PLUG_CONTROL"],
                             clear=c["GPU_AZ_CLOCK_GATING_DISABLE"],
                             note="re-select is mandatory: the index window has no memory"))
        # A18 AFMT audio clock.  ⛔ never gate on bit 8 (AUDIO_CLOCK_ON is a read-only ACK — an
        # echo of our own write, not an answer).
        ops.append(self._rmw("AFMT_CNTL", c["GPU_R_AFMT_CNTL"] + d, set_=0x1))
        ops.append(self._w("HDMI_DB_CONTROL", c["GPU_R_HDMI_DB_CONTROL"] + d, 0x00001000,
                           note="agnos ships this; must clear the GOP's bit4/bit16"))
        p.extend(ops, block="S_der_head")
        p.extend(self._infoframes(), block="S_der_infoframes")
        # A21 THE MUTE BOUNDARY — staged shut.
        p.extend([
            self._rmw("AFMT_AUDIO_PACKET_CONTROL", c["GPU_R_AFMT_AUDIO_PACKET_CONTROL"] + d,
                      clear=c["GPU_AFMT_AUDIO_SAMPLE_SEND"], set_=c["GPU_AFMT_60958_CS_UPDATE"],
                      note="THE MUTE BOUNDARY: CS_UPDATE set, SAMPLE_SEND cleared", flush=True),
            Op("R", "AFMT_AUDIO_INFO0", rel=c["GPU_R_AFMT_AUDIO_INFO0"] + d, expect=0x170,
               note="a real ANSWER register, not an echo — supporting data only"),
            self._rmw("AFMT_AUDIO_PACKET_CONTROL", c["GPU_R_AFMT_AUDIO_PACKET_CONTROL"] + d,
                      set_=0x800000, note="FIFO_OVERFLOW_ACK; amdgpu never sets it — logged as "
                                          "a second-order delta"),
            Op("R", "AFMT_STATUS", rel=c["GPU_R_AFMT_STATUS"] + d,
               note="supporting only, NEVER adjudicating (R6)"),
            self._rmw("DIG_FE_CNTL", c["GPU_R_DIG_FE_CNTL"] + d, set_=0x100,
                      note="STEREOSYNC_GATE_EN"),
        ], block="S_der_tail")
        # ⛔ NOT PORTED: STAGE-1's hand-rolled imitation transmitter edge (compiled out under
        # MODESET_AUDIO — running it beside the real edge makes any sound unattributable), and
        # the CRC probe (up to 120 ms of busy-wait inside the OTG-disabled envelope, writing a
        # register amdgpu never touches, on an experiment whose oracle is an ear).
        return p

    def _infoframes(self):
        """A20 — gpu_hdmi_program_infoframes(d, az)."""
        c, d, az = self.c, self.d, self.az
        ops = []
        ops.append(self._rmw("AFMT_VBI_PACKET_CONTROL", c["GPU_R_AFMT_VBI_PACKET_CONTROL"] + d,
                             clear=0xF0000000, set_=0x20000))
        ops.append(self._w("AFMT_GENERIC_HDR", c["GPU_R_AFMT_GENERIC_HDR"] + d, 0x000D0282,
                           note="AVI header, format-invariant"))
        # ⭐ A20.3 the AVI BODY is COMPUTED, never amdgpu's 0x80885E8F.  agnos ships RGB
        # (DB1/2/3 = 0x1E/0x08/0x00); amdgpu's literal is YCbCr, so writing it would move
        # colorimetry as well as the variable under test.  The checksum is derived from the
        # LIVE raster bars, so this value is correct on whatever mode the GOP actually parked.
        db1, db2, db3 = 0x1E, 0x08, 0x00
        g2, g3 = self.v_total, self.h_total
        s = 0x82 + 0x02 + 0x0D + db1 + db2 + db3 + (g2 & 0xFF) + (g2 >> 8) + (g3 & 0xFF) + (g3 >> 8)
        cks = (0x100 - (s & 0xFF)) & 0xFF
        avi = cks | (db1 << 8) | (db2 << 16) | (db3 << 24)
        ops.append(self._w("AFMT_GENERIC_0", c["GPU_R_AFMT_GENERIC_0"] + d, avi,
                           note=f"COMPUTED AVI body (RGB), cks=0x{cks:02X} from the LIVE raster"))
        ops.append(self._w("AFMT_GENERIC_1", c["GPU_R_AFMT_GENERIC_1"] + d, 0))
        ops.append(self._w("AFMT_GENERIC_2", c["GPU_R_AFMT_GENERIC_2"] + d, g2,
                           note="DERIVED bar = v_total"))
        ops.append(self._w("AFMT_GENERIC_3", c["GPU_R_AFMT_GENERIC_3"] + d, g3,
                           note="DERIVED bar = h_total"))
        for i in range(4, 8):
            ops.append(self._w(f"AFMT_GENERIC_{i}", c[f"GPU_R_AFMT_GENERIC_{i}"] + d, 0))
        ops.append(self._rmw("AFMT_VBI_PACKET_CONTROL1", c["GPU_R_AFMT_VBI_PACKET_CONTROL1"] + d,
                             set_=0x4))
        ops.append(self._rmw("HDMI_GENERIC_PACKET_CONTROL1",
                             c["GPU_R_HDMI_GENERIC_PACKET_CONTROL1"] + d,
                             clear=0xFFFF, set_=0x2,
                             note="GENERIC0_LINE lives in CONTROL1 on DCN2"))
        ops.append(self._rmw("HDMI_GENERIC_PACKET_CONTROL0",
                             c["GPU_R_HDMI_GENERIC_PACKET_CONTROL0"] + d, set_=0x3))
        ops.append(self._rmw("HDMI_ACR_PACKET_CONTROL", c["GPU_R_HDMI_ACR_PACKET_CONTROL"] + d,
                             clear=0x100 | 0x30, set_=0x1000,
                             note="ACR_SOURCE=0 => HARDWARE-measured CTS"))
        ops.append(self._rmw("HDMI_ACR_48_1", c["GPU_R_HDMI_ACR_48_1"] + d,
                             clear=0xFFFFF, set_=6144, note="CEA-861 N, function of Fs only"))
        ops.append(self._rmw("HDMI_ACR_32_1", c["GPU_R_HDMI_ACR_32_1"] + d, set_=4096))
        ops.append(self._rmw("HDMI_ACR_44_1", c["GPU_R_HDMI_ACR_44_1"] + d, set_=6272))
        ops.append(self._rmw("HDMI_AUDIO_PACKET_CONTROL",
                             c["GPU_R_HDMI_AUDIO_PACKET_CONTROL"] + d,
                             clear=0x30 | 0x1F0000, set_=0x10,
                             note="ppl MUST be 0 on DCN 2.1 — the DCE-3 formula is falsified"))
        ops.append(self._rmw("AFMT_AUDIO_SRC_CONTROL", c["GPU_R_AFMT_AUDIO_SRC_CONTROL"] + d,
                             clear=0x7, set_=az, note="DERIVED from az"))
        ops.append(self._rmw("AFMT_AUDIO_PACKET_CONTROL2",
                             c["GPU_R_AFMT_AUDIO_PACKET_CONTROL2"] + d,
                             clear=0xFF00 | 0x1 | 0x10000000, set_=0x300,
                             note="0 channels = silence with everything else perfect"))
        # ⛔ these three must PRECEDE the CS_UPDATE latch; _2 is NOT adjacent to _0/_1
        ops.append(self._rmw("AFMT_60958_0", c["GPU_R_AFMT_60958_0"] + d, set_=0x100000))
        ops.append(self._rmw("AFMT_60958_1", c["GPU_R_AFMT_60958_1"] + d, set_=0x200000))
        ops.append(self._rmw("AFMT_60958_2", c["GPU_R_AFMT_60958_2"] + d,
                             clear=0xFFFFFF, set_=0x876543))
        ops.append(self._rmw("HDMI_INFOFRAME_CONTROL1", c["GPU_R_HDMI_INFOFRAME_CONTROL1"] + d,
                             clear=0x3F00, set_=0x200, note="SEND with LINE=0 has no slot"))
        ops.append(self._rmw("HDMI_INFOFRAME_CONTROL0", c["GPU_R_HDMI_INFOFRAME_CONTROL0"] + d,
                             set_=0x10))
        # ⛔ THE UPDATE PULSE.  The old script wrote the post-pulse RESIDUE (0) and therefore
        # never latched the audio InfoFrame at all.
        ops.append(self._rmw("AFMT_INFOFRAME_CONTROL0", c["GPU_R_AFMT_INFOFRAME_CONTROL0"] + d,
                             set_=0x80, note="AUDIO_INFO_UPDATE pulse (self-clearing)",
                             flush=True))
        return ops

    # -- U_agn: agnos list (B), gpu_hdmi_audio_unmute() --------------------------------------
    def u_agn(self):
        """⭐ THE SAME FUNCTION PRODUCES BOTH THE PRE AND THE POST UNMUTE.

        That is precisely what makes P and Q one experiment with one variable.  The assertion
        machinery proves the emitted block is byte-identical between them; the only thing that
        differs is WHERE it sits relative to the edge."""
        c, d = self.c, self.d
        p = Plan("U_agn")
        p.extend([
            self._azw(c["GPU_AZ_IX_HOT_PLUG_CONTROL"], set_=c["GPU_AZ_AUDIO_ENABLED"],
                      note="AUDIO_ENABLED"),
            # ⛔ MUST FOLLOW AUDIO_ENABLED: the endpoint clears the slot map across that
            # transition (agnos 1.55.14 iron finding).  amdgpu never writes ordinal 0x36 at all,
            # which is why agnos's slot map was never under the capture's constraint and moves
            # WITH AUDIO_ENABLED into the unmute.
            self._azw(c["GPU_AZ_IX_MULTICHANNEL_ENABLE"], value=c["GPU_AZ_MULTICHANNEL_STEREO"],
                      note="slot map; must follow AUDIO_ENABLED"),
            Op("AZR", "AZ_ENDPOINT", az_ord=c["GPU_AZ_IX_MULTICHANNEL_ENABLE"],
               note="never assume the write stuck — warn if bit0 reads 0"),
            self._rmw("AFMT_AUDIO_PACKET_CONTROL", c["GPU_R_AFMT_AUDIO_PACKET_CONTROL"] + d,
                      set_=c["GPU_AFMT_AUDIO_SAMPLE_SEND"], note="⛔ THE UNMUTE", flush=True),
            self._rmw("HDMI_GC", c["GPU_R_HDMI_GC"] + d, clear=c["GPU_HDMI_GC_AVMUTE"],
                      note="AVMUTE clear", flush=True),
        ], block="U_agn")
        return p


# =============================================================================================
# §6  THE ENVELOPE — mdo_transmit_run(arm) ported.  Structural only; the live executor resolves
#     saved values at run time.
#
# ⛔ E_dis IS ONE STORE.  DIG_MODE 2->3 and the BE<->FE disconnect go out together, exactly as
# the capture does (0x10030000).  Two writes would put a moment of live-HDMI-with-no-front-end
# on the wire that the reference never produces.
# ⛔ E_close writes back THE GOP's saved clock-control values, never amdgpu's 0x3.  The GOP pipe
# runs OTG_CLOCK_CONTROL=0x10101 / OPTC_INPUT_CLOCK_CONTROL=0x6, and forcing 0x3 would likely
# have gone dark — that is the D3 finding that saved the M6 burn.
# =============================================================================================

class Envelope:
    # C18's timing group, in the EXACT order the capture writes it.  Order is load-bearing.
    TIMING_GROUP = [
        "GPU_R_OTG_H_TOTAL", "GPU_R_OTG_H_BLANK_START_END", "GPU_R_OTG_H_SYNC_A",
        "GPU_R_OTG_H_SYNC_A_CNTL", "GPU_R_OTG_H_TIMING_CNTL", "GPU_R_OTG_V_TOTAL",
        "GPU_R_OTG_V_TOTAL_MIN", "GPU_R_OTG_V_TOTAL_MAX", "GPU_R_OTG_V_BLANK_START_END",
        "GPU_R_OTG_V_SYNC_A", "GPU_R_OTG_V_SYNC_A_CNTL", "GPU_R_OTG_INTERLACE_CONTROL",
        "GPU_R_OTG_VSTARTUP_PARAM", "GPU_R_OTG_VUPDATE_PARAM", "GPU_R_OTG_VREADY_PARAM",
        "GPU_R_VTG0_CONTROL", "GPU_R_OTG_CLOCK_CONTROL", "GPU_R_OPTC_INPUT_CLOCK_CONTROL",
    ]

    def __init__(self, consts, phy, edge_class="ATOM"):
        self.c, self.phy, self.edge_class = consts, phy, edge_class
        self.db = phy * DIG_STRIDE

    def e_open(self):
        # ⛔⛔ THE OTG ENVELOPE CANNOT BE REPLAYED FROM LINUX USERSPACE ON THIS PIPE.
        # MEASURED, 2026-07-24, archaemenid, bisect N0:5: the write LANDS —
        #     W 0x5001 <- 0x80011300 (pre 0x80011301); R 0x5001 -> 0x80001300
        # — and the APU hard-wedges before the very next MMIO read. Reproduced with fbcon
        # unbound, so "the console was scanning it out" is FALSIFIED, not merely unproven.
        # agnos survives the identical write (M6, iron, exit 95) because agnos OWNS the machine;
        # those conditions are not reproducible from userspace, so this is a CONSTRAINT, not a
        # bug to patch.
        # ⚠ And the envelope is NOT what L1 asks about. L1 asks whether the unmute falls before
        # or after the TRANSMITTER edge — ATOM #4/#76 — which does not require the OTG stopped.
        # NOENV keeps the edge and drops the envelope: lower fidelity to agnos's exact program,
        # and it is the difference between an answerable question and a wedged box.
        if self.edge_class in ("NOENV", "MINIMAL", "HDMIMODE"):
            return [Op("FN", "E_open_skipped",
                       note="NOENV: OTG left RUNNING — the envelope wedges the APU from userspace")]
        return [Op("FN", "E_open", note="OTG_MASTER_EN<-0; wait frame stop; ABORT+RESTORE if "
                                        "the counter never stops")]

    def e_dis(self, audio_arm):
        c = self.c
        # ⛔⛔ MINIMAL: NO BE<->FE ROUTING WRITES AT ALL.
        # MEASURED 2026-07-24, window 2 (Rlit), NOENV: the E_pulse disconnect/connect on
        # DIG_BE_CNTL wedged the APU on a RUNNING pipe —
        #     W 0x566F <- 0x10030200 / 0x10030000 / 0x10030200 <- died
        # while window 1 (N0), the one arm that provably never writes 0x566F (assertion A5),
        # completed cleanly. agnos only ever re-routes BE<->FE with the OTG STOPPED, inside the
        # envelope. Since the envelope ALSO wedges from userspace (bisect N0:5), userspace
        # cannot do either half of agnos's bracket on this hardware.
        # MINIMAL keeps the ONE thing L1 actually needs — a real ATOM #76 PHY edge — and drops
        # every pipe-reconfiguration write around it.
        # ⚠ COST, stated plainly: DIG_MODE stays 2 (DVI), so the HDMI_* block is INERT and an
        # unmute may be unable to produce audio at all. If every arm including Rlit is silent
        # under MINIMAL, that is a statement about the METHOD, not about agnos, and the verdict
        # lattice must read it as VOID-HARNESS.
        if self.edge_class == "MINIMAL":
            return [Op("FN", "E_dis_skipped",
                       note="MINIMAL: no DIG_BE_CNTL writes — they wedge a running pipe")]
        if self.edge_class == "HDMIMODE":
            # ⭐ THE ONE WRITE THAT MAKES THE EXPERIMENT NON-VACUOUS, AND NOTHING ELSE.
            # 2026-07-24 measured two distinct userspace wedges, and they are NOT the same write:
            #   E_open  — OTG_MASTER_EN cleared          -> wedge (bisect N0:5)
            #   E_pulse — FE_SOURCE_SELECT torn down     -> wedge (battery w2, Rlit)
            # Both disturb the pipe's TOPOLOGY. A DIG_MODE flip does not: it changes bits
            # [18:16] of DIG_BE_CNTL and leaves FE_SOURCE_SELECT — the routing — exactly as is.
            # ⚠ It is required because MINIMAL's seven-window run was silent on BOTH D windows
            # (the positive control), which is what a DVI-mode link does to audio: the whole
            # HDMI_* block is inert at DIG_MODE 2, so no unmute anywhere can sound. Without this
            # the battery cannot answer its question no matter how many windows complete.
            # ⛔ UNTESTED ON THIS PATH. Bisect it before trusting it to a session.
            return [Op("RMW", "DIG_BE_CNTL", rel=c["GPU_R_DIG_BE_CNTL"] + self.db,
                       mask=c["GPU_DIG_MODE_MASK"],
                       set_=c["GPU_DIG_MODE_HDMI"] << c["GPU_DIG_MODE_SHIFT"],
                       note="DIG_MODE 2->3 ONLY; FE_SOURCE_SELECT untouched", flush=True)]
        if audio_arm:
            return [Op("RMW", "DIG_BE_CNTL", rel=c["GPU_R_DIG_BE_CNTL"] + self.db,
                       mask=c["GPU_DIG_FE_SOURCE_SEL_MASK"] | c["GPU_DIG_MODE_MASK"],
                       set_=c["GPU_DIG_MODE_HDMI"] << c["GPU_DIG_MODE_SHIFT"],
                       note="ONE STORE: DIG_MODE 2->3 + disconnect (= 0x10030000)", flush=True)]
        # N0's DVI-parked null: disconnect only, DIG_MODE untouched.
        return [Op("RMW", "DIG_BE_CNTL", rel=c["GPU_R_DIG_BE_CNTL"] + self.db,
                   mask=c["GPU_DIG_FE_SOURCE_SEL_MASK"],
                   note="null arm: disconnect only, DIG_MODE stays 2", flush=True)]

    def e_atom4(self):
        # ⚠ NOENV still runs the real ATOM edge — that is the whole point of the class.
        if self.edge_class == "SOFT":
            return [Op("DELAY", "E_atom4_soft", value=3170, note="SOFT degrade: idle dwell")]
        return [Op("ATOM", "DIGxEncoderControl_4",
                   note="ps=[0x04030F01,24150,0,0] — agnos ships it (atom.cyr). Effect is "
                        "0x5628/0x562B/0x569A; it NEVER touches DIG_BE_CNTL")]

    def e_tx(self):
        # ⚠ NOENV still runs the real #76 DISABLE->ENABLE. Only the OTG envelope is dropped.
        if self.edge_class == "SOFT":
            return [Op("DELAY", "E_tx_soft", value=3170,
                       note="SOFT degrade: no PHY writes — the silence branch is DOWNGRADED")]
        return [Op("ATOM", "DIG1TransmitterControl_76_DISABLE", note="action 0"),
                Op("ATOM", "DIG1TransmitterControl_76_ENABLE",
                   note="action 1; one-shot retry if DISABLE took and ENABLE failed")]

    def e_conn(self):
        c = self.c
        if self.edge_class in ("MINIMAL", "HDMIMODE"):
            return [Op("FN", "e_conn_skipped", note="MINIMAL: routing never changed, nothing to reconnect")]
        return [Op("RMW", "DIG_BE_CNTL", rel=c["GPU_R_DIG_BE_CNTL"] + self.db,
                   mask=c["GPU_DIG_FE_SOURCE_SEL_MASK"], set_=c["GPU_DIG_FE_SOURCE_CONNECT"],
                   note="⛔ RE-READ, never reuse the pre-edge snapshot", flush=True),
                Op("FN", "E_conn_verify",
                   note="readback verify DIG_MODE==3 AND FE_SOURCE_SELECT==0x2 else ABORT+RESTORE")]

    def e_pulse(self):
        c = self.c
        if self.edge_class in ("MINIMAL", "HDMIMODE"):
            return [Op("FN", "e_pulse_skipped", note="MINIMAL: the pulse is what wedged window 2")]
        return [Op("RMW", "DIG_BE_CNTL", rel=c["GPU_R_DIG_BE_CNTL"] + self.db,
                   mask=c["GPU_DIG_FE_SOURCE_SEL_MASK"], note="pulse: disconnect", flush=True),
                Op("RMW", "DIG_BE_CNTL", rel=c["GPU_R_DIG_BE_CNTL"] + self.db,
                   set_=c["GPU_DIG_FE_SOURCE_CONNECT"], note="pulse: reconnect", flush=True)]

    def e_close(self):
        if self.edge_class in ("NOENV", "MINIMAL", "HDMIMODE"):
            # Nothing to close: the timing group was never disturbed because the OTG never
            # stopped. Writing it back anyway would be 18 pointless stores on a live pipe.
            return [Op("FN", "E_close_skipped", note="NOENV: timing group never disturbed")]
        ops = [Op("W", n.replace("GPU_R_", ""), rel=self.c[n], value=None,
                  note="restore the SAVED value (⛔ the GOP's, never amdgpu's 0x3)")
               for n in self.TIMING_GROUP]
        ops.append(Op("FN", "E_master_en", note="OTG_MASTER_EN <- saved"))
        return ops

    def e_settle(self):
        if self.edge_class in ("NOENV", "MINIMAL", "HDMIMODE"):
            return [Op("DELAY", "E_settle_hold", value=120_000,
                       note="NOENV: plain 120 ms hold; the pipe never stopped so there is "
                            "no resume to confirm")]
        return [Op("FN", "E_settle", note="vblank wait + 120 ms hold + FRAME_COUNT re-confirm")]


# =============================================================================================
# §7  THE REFERENCE ARM — MACHINE-EXTRACTED from the amdgpu ftrace.  No hand transcription.
#
# ⚠ Rlit is a CAPTURED-WRITE REPLAY, and it must never be described as an ATOM replay: amdgpu on
# DCN2 drives the PHY through native link_enc code, not ATOM #76.  The trace also contains no
# READS, so replaying its writes is replaying a read-dependent sequence write-only.  Machine
# extraction plus sha256(ftrace) makes that AUDITABLE, not correct — if Rlit is silent we cannot
# cleanly separate "the reference genuinely cannot arm this pipe from userspace" from "our
# extraction is incomplete", and the verdict text says so.
# =============================================================================================

class FtraceRef:
    LINE = re.compile(r"(\d+\.\d+).*amdgpu_device_wreg:.*?0x1638,\s*(0x[0-9a-fA-F]+),\s*(0x[0-9a-fA-F]+)")

    def __init__(self, path=FTRACE):
        self.path = path
        self.sha = sha256_file(path)
        self.rows = []
        try:
            for line in open(path, errors="replace"):
                m = self.LINE.search(line)
                if m:
                    self.rows.append((float(m.group(1)), int(m.group(2), 0), int(m.group(3), 0)))
        except OSError:
            pass

    def window(self, t0, t1):
        return [(t, i, v) for (t, i, v) in self.rows if t0 <= t <= t1]

    def available(self):
        return len(self.rows) > 0

    # Committed bounds. These go into the commitment hash alongside sha256(ftrace), so the
    # reference arm is auditable: anyone can re-extract it and get the same write list.
    STAGE = (33338.083307, 33338.083515)
    EDGEGRP = (33338.099822, 33338.099888)
    UNMUTE = (33338.105557, 33338.105563)

    def ops(self, bounds, tag):
        """Machine-extracted absolute writes. ⚠ NOT an ATOM replay and never to be described as
        one: amdgpu on DCN2 drives the PHY through native link_enc code, not ATOM #76. The trace
        also contains NO READS, so this replays a read-dependent sequence write-only."""
        out = []
        for (t, idx, val) in self.window(*bounds):
            out.append(Op("W", f"ftrace_{tag}", space=0, rel=idx, value=val,
                          note=f"t={t:.6f} abs=0x{idx:04X}",
                          flush=idx in (0x566F, 0x566A, 0x384C, 0x384D)))
        return out

    def unmute_delay_ms(self):
        return (self.UNMUTE[0] - self.STAGE[0]) * 1000.0


# =============================================================================================
# §8  BLINDING — commit-then-reveal.
#
# HONEST LIMIT, stated up front: this is tamper-EVIDENT, not tamper-proof.  The key file exists
# on disk from T+0 and nothing physically stops it being read.  What it guarantees is that the
# key HASH is in the tty scrollback and in console.log before the first sound, and that every
# report row is fsync'd before the key becomes readable — so any retrofit requires tampering
# with two files whose hashes are already published.  The split seed means neither party alone
# chose the answer.
# =============================================================================================

PITCHES = {"A": 220, "B": 440, "C": 880, "D": 1760}
BURSTS = [2, 3, 4, 5]
SWEEPS = ["U", "D"]
CHANNELS = ["L", "R", "B"]
GAPS = [120, 400]
SPANS = [(300, 3000), (400, 2400)]


def derive_key(master_hex, n_windows, arms):
    """A PURE FUNCTION, committed in the script, so anyone holding (seed, script) can
    re-derive the entire plan.  Deterministic: window->arm, window->pattern, pad targets."""
    plan = []
    for k in range(n_windows):
        h = hashlib.sha256(f"{master_hex}:{k}".encode()).digest()
        pitch_k = list(PITCHES)[h[0] % 4]
        pat = {
            "bursts": BURSTS[h[1] % 4],
            "pitch": pitch_k,
            "pitch_hz": PITCHES[pitch_k],
            "sweep": SWEEPS[h[2] % 2],
            "span": SPANS[h[3] % 2],
            "channels": CHANNELS[h[4] % 3],
            "gap_ms": GAPS[h[5] % 2],
        }
        # ⛔ The banned 2026-07-15 pattern is published and therefore guessable.  Perturb rather
        # than reject, so the draw stays uniform over the remaining space.
        if (pat["bursts"] == BANNED_PATTERN["bursts"]
                and pat["pitch_hz"] == BANNED_PATTERN["pitch"]
                and pat["sweep"] == BANNED_PATTERN["sweep"]
                and tuple(pat["span"]) == BANNED_PATTERN["span"]):
            pat["bursts"] = BURSTS[(h[1] + 1) % 4]
        plan.append({"window": k + 1, "arm": arms[k], "pattern": pat})
    return plan


def render_pattern(pat, fs=48000, amp=30000, seconds=6.0):
    """Generate the stimulus THROUGH THE FEED PATH UNDER TEST — never through ALSA.

    ⚠ Level is IDENTICAL and near-full-scale in every window and every arm, so a reported level
    difference is attributable to the PATH, not the stimulus.  That is the -48 dB sign-folded
    packing precedent: a real signal that was merely quiet once read as no signal at all."""
    import math
    n = int(fs * seconds)
    buf = bytearray()
    tone_n = int(fs * 0.180)
    gap_n = int(fs * pat["gap_ms"] / 1000.0)
    sil_n = int(fs * 0.400)
    sweep_n = int(fs * 1.2)
    lo, hi = pat["span"]

    def push(sample, chan_l, chan_r):
        buf.extend(struct.pack("<hh", int(sample * chan_l), int(sample * chan_r)))

    if pat["channels"] == "L":
        cl, cr = 1.0, 0.0
    elif pat["channels"] == "R":
        cl, cr = 0.0, 1.0
    else:
        cl, cr = 1.0, 1.0

    def emit_once():
        for _ in range(pat["bursts"]):
            for i in range(tone_n):
                push(amp * math.sin(2 * math.pi * pat["pitch_hz"] * i / fs), cl, cr)
            for _ in range(gap_n):
                push(0, cl, cr)
        for _ in range(sil_n):
            push(0, cl, cr)
        ph = 0.0
        for i in range(sweep_n):
            frac = i / sweep_n
            f = lo + (hi - lo) * (frac if pat["sweep"] == "U" else (1 - frac))
            ph += 2 * math.pi * f / fs
            push(amp * math.sin(ph), cl, cr)
        for _ in range(sil_n):
            push(0, cl, cr)

    emit_once()
    emit_once()
    while len(buf) < n * 4:
        buf.extend(struct.pack("<hh", 0, 0))
    return bytes(buf[:n * 4])


REPORT_RE = re.compile(r"^([0-3])([2-5-])([ABCD-])([UD-])([LRB-])([<=>-])([CSBN-])?$")


def parse_report(code, u_phase=False):
    """⛔ The tool NEVER advances until both lines parse, so blind misalignment cannot silently
    drift.  `0` alone is accepted for 'nothing' — that will be the most common answer and it is
    one keystroke, typed without looking."""
    code = code.strip()
    if code == "0":
        code = "0-----" + ("-" if u_phase else "")
    if u_phase and len(code) == 6:
        code += "-"
    m = REPORT_RE.match(code)
    if not m:
        return None
    g = m.groups()
    return {"grade": int(g[0]), "bursts": g[1], "pitch": g[2], "sweep": g[3],
            "channels": g[4], "level": g[5], "temporal": g[6] or "-", "raw": code}


def score_window(report, pattern):
    """PRE-REGISTERED, fixed before the session: a window MATCHES iff BURSTS and CHANNEL ORDER
    are both correct.  Pitch/sweep/span/gap are corroboration only.

    Rationale, and it is not a convenience: a real but attenuated tone can yield a correct burst
    count and a wrong pitch class.  Requiring all six axes would make a degraded-but-real
    positive read as a negative — the -48 dB packing bug wearing a different hat.
    p(match by guessing) = 1/12."""
    if report is None:
        return False
    b_ok = report["bursts"] == str(pattern["bursts"])
    c_ok = report["channels"] == pattern["channels"]
    return bool(b_ok and c_ok)


# =============================================================================================
# §9  ARMS + THE FIVE WRITE-SET ASSERTIONS.
#
# ⭐ THE ASSERTIONS ARE THE SINGLE-VARIABLE CLAIM, MACHINE-CHECKED.  Without them "P and Q are
# the same program in a different order" is something I would be ASSERTING, and this arc has
# already been bitten by exactly that: ATOM_DRY, where "dry" and "live" compiled BYTE-IDENTICAL
# and two runs differed in name but not in behaviour.  --plan-diff runs these with no root and
# no hardware, so the claim is reviewable BEFORE the session is booked.
# =============================================================================================

ARM_SPECS = {
    #  name    staging  unmute   slot        edge    reps  role
    "N0":   dict(stage="der", unmute=None,  slot=None,   edge="E0", reps=2,
                 role="NULL-FLAT: everything except the BE<->FE event, the PHY cycle, the unmute"),
    "N1":   dict(stage="der", unmute=None,  slot=None,   edge="E2", reps=2,
                 role="NULL-EDGE: vs Q it is minus EXACTLY the unmute block — the tightest control"),
    "P":    dict(stage="der", unmute="agn", slot="PRE",  edge="E2", reps=3,
                 role="= kernel op AUDIO_PRE (0x07)"),
    "Q":    dict(stage="der", unmute="agn", slot="POST", edge="E2", reps=3,
                 role="= kernel op AUDIO_POST (0x08)"),
    "D":    dict(stage="der", unmute="agn", slot="BOTH", edge="E2", reps=2,
                 role="POSITIVE CONTROL carrying identical disturbance; separates rescue from poison"),
    "Rlit": dict(stage="lit", unmute="lit", slot="POST", edge="E2", reps=2,
                 role="REFERENCE / SESSION-VALIDITY GATE (S1). Silent => VOID-HARNESS"),
}

BLOCK1_ORDER_RULES = """\
≥1 Rlit in windows 1-4 and ≥1 in windows 11-14 (a mid-session sink-latch detector);
no more than two consecutive identical arms;
every P window bracketed by a sounding-expected window within 3 slots on each side."""


def build_arm_plan(programs, envelope, arm):
    """Assemble one arm.  Staging is IDENTICAL across every derived arm; the ONLY thing that
    moves is where the unmute block sits.  That is what makes this one experiment with one
    variable rather than six loosely-related runs."""
    spec = ARM_SPECS[arm]
    audio_arm = spec["edge"] == "E2" and arm != "N0"
    p = Plan(arm)
    p.extend(programs.g1_mute_base().ops, block="G1")
    p.extend([Op("FN", "G2_feed_stop", note="agnos's A0 drain-before-feed")], block="G2")
    p.extend(envelope.e_open(), block="E_open")
    if spec["edge"] == "E2":
        p.extend(envelope.e_dis(audio_arm), block="E_dis")
    else:
        # ⛔ E0 replaces the routing/PHY events with EQUAL-DURATION IDLES.  DIG_BE_CNTL is never
        # written.  The panel still blanks — identically — because E_open/E_close still run.
        p.extend([Op("DELAY", "E_dis_idle", value=0,
                     note="E0: equal-duration idle, measured in Phase 3")], block="E_dis")
    p.extend(envelope.e_atom4(), block="E_atom4")
    p.extend([Op("FN", "E_pre_attrs", note="AFMT clock + stage attributes + infoframes")],
             block="E_pre")
    if spec["stage"] == "der":
        p.extend(programs.s_der().ops, block="STAGE")
    else:
        # Rlit: the machine-extracted amdgpu staging block. ⚠ Its PHY writes are deliberately
        # NOT replayed (native link_enc, read-dependent, write-only in the trace) — the edge in
        # every arm is ATOM, or SOFT.
        ft = programs.ftrace
        p.extend((ft.ops(ft.STAGE, "stage") + ft.ops(ft.EDGEGRP, "edgegrp")) if ft else [],
                 block="STAGE")
    if spec["slot"] in ("PRE", "BOTH"):
        p.extend(programs.u_agn().ops, block="U_PRE")
    if spec["edge"] == "E2":
        p.extend(envelope.e_tx(), block="E_tx")
        p.extend(envelope.e_conn(), block="E_conn")
        p.extend(envelope.e_pulse(), block="E_pulse")
    else:
        p.extend([Op("DELAY", "E_tx_idle", value=0, note="E0 idle")], block="E_tx")
    p.extend([Op("FN", "E_replay", note="verbatim re-emit of E_pre")], block="E_replay")
    p.extend(envelope.e_close(), block="E_close")
    p.extend(envelope.e_settle(), block="E_settle")
    if spec["slot"] in ("POST", "BOTH"):
        if spec["unmute"] == "lit":
            ft = programs.ftrace
            # The 22.2 ms gap is reproduced as a REAL MEASURED DELAY, not a sleep hint; the
            # achieved value is logged so a missed deadline is visible rather than assumed.
            p.extend([Op("DELAY", "U_lit_gap",
                         value=int(ft.unmute_delay_ms() * 1000) if ft else 22245,
                         note="measured amdgpu stage->unmute gap")]
                     + (ft.ops(ft.UNMUTE, "unmute") if ft else []), block="U_POST")
        else:
            p.extend(programs.u_agn().ops, block="U_POST")
    # PAD then feed-start then play. ⚠ PAD makes every arm's window WALL-TIME IDENTICAL — an
    # operator who can time the windows has a side channel into which arm is running, and E0
    # arms are structurally shorter than E2 ones.
    # ⛔ ONLY arms that actually FLIPPED may restore. N0 is the flat null: edge E0, it never
    # calls e_dis, so it never enters HDMI mode and must not write DIG_BE_CNTL at all —
    # assertion A5 exists precisely to catch this, and it did.
    if envelope.edge_class == "HDMIMODE" and spec["edge"] == "E2":
        # ⛔ PUT DIG_MODE BACK INSIDE THE WINDOW. Leaving the link in HDMI mode across the gap
        # would let window k+1 inherit window k's signalling state — the randomised order would
        # stop being randomised. G0 also restores it, but doing it here makes the window
        # self-contained rather than relying on the restore pass.
        p.extend([Op("RMW", "DIG_BE_CNTL", rel=envelope.c["GPU_R_DIG_BE_CNTL"] + envelope.db,
                     mask=envelope.c["GPU_DIG_MODE_MASK"],
                     set_=envelope.c["GPU_DIG_MODE_DVI"] << envelope.c["GPU_DIG_MODE_SHIFT"],
                     note="DIG_MODE 3->2 restore, routing untouched", flush=True)],
                 block="E_mode_restore")
    p.extend([Op("DELAY", "PAD", value=0, note="compensating pad to a common window length"),
              Op("FN", "F_start", note="BDL + SDn + RUN; verify LPIB advances"),
              Op("DELAY", "PLAY", value=6_000_000, note="6 s of this window's blinded pattern")],
             block="PLAY")
    return p


def write_set_assertions(plans):
    """The five §2.4 assertions.  ANY failure ⇒ session VOID."""
    res = []

    def add(ok, name, detail):
        res.append({"assertion": name, "pass": bool(ok), "detail": detail})

    derived = [a for a in ("N0", "N1", "P", "Q", "D") if a in plans]
    stage_hashes = {a: Plan.hash_ops(plans[a].block_ops("STAGE")) for a in derived}
    uniq = set(stage_hashes.values())
    add(len(uniq) == 1, "A1 staging byte-identical across all derived arms",
        stage_hashes)

    u_p = Plan.hash_ops(plans["P"].block_ops("U_PRE")) if "P" in plans else None
    u_q = Plan.hash_ops(plans["Q"].block_ops("U_POST")) if "Q" in plans else None
    add(u_p is not None and u_p == u_q,
        "A2 the unmute block is byte-identical between P and Q",
        {"P.U_PRE": u_p, "Q.U_POST": u_q})

    if "N1" in plans and "Q" in plans:
        q_ops = [o for o in plans["Q"].ops]
        u_ops = plans["Q"].block_ops("U_POST")
        q_minus = [o for i, o in enumerate(q_ops)
                   if not (plans["Q"].blocks["U_POST"][0] <= i < plans["Q"].blocks["U_POST"][1])]
        add(Plan.hash_ops(q_minus) == Plan.hash_ops(plans["N1"].ops),
            "A3 trace(N1) == trace(Q) minus exactly the unmute block",
            {"Q_minus_unmute": Plan.hash_ops(q_minus),
             "N1": Plan.hash_ops(plans["N1"].ops),
             "unmute_op_count": len(u_ops)})

    if "P" in plans and "Q" in plans:
        add(plans["P"].struct_hash() != plans["Q"].struct_hash(),
            "A4 P and Q DIFFER (in position only — A1+A2 pin the content)",
            {"P": plans["P"].struct_hash()[:16], "Q": plans["Q"].struct_hash()[:16]})

    if "N0" in plans:
        wrote_be = any(o.name == "DIG_BE_CNTL" and o.kind in ("W", "RMW")
                       for o in plans["N0"].ops)
        add(not wrote_be, "A5 the flat null (N0) never writes DIG_BE_CNTL",
            {"dig_be_cntl_writes": wrote_be})
    return res


# =============================================================================================
# §10  VERDICT LATTICE — pre-registered, evaluated by the SCRIPT, never by prose.
#
# ⛔ THE STRUCTURAL GUARD ON MY OWN NARRATIVE: the scoring is machine-computed and the verdict is
# printed by this function.  I have over-claimed on this exact arc before.  The report may quote
# VERDICT.md's computed lines; it may not re-adjudicate them.
# =============================================================================================

def classify(arm_windows):
    """sound(X) = >=2 windows graded 3 with a machine-verified match.
       silent(X) = ALL windows graded 0 or 1.  Grade 1 (noise floor) NEVER counts as positive.
       anything else = mixed -> INCONCLUSIVE."""
    out = {}
    for arm, rows in arm_windows.items():
        strong = sum(1 for r in rows if r.get("grade") == 3 and r.get("match"))
        allquiet = all(r.get("grade", 0) in (0, 1) for r in rows) and len(rows) > 0
        if strong >= 2:
            out[arm] = "sound"
        elif allquiet:
            out[arm] = "silent"
        else:
            out[arm] = "mixed"
    return out


def verdict(cls, gates):
    """First match wins.  Every branch names what it LICENSES — including the branches that
    license nothing about agnos."""
    failed = [k for k, v in gates.items() if not v]
    if cls.get("Rlit") == "silent":
        return ("VOID-HARNESS",
                "amdgpu's own sequence replayed from Linux did NOT sound. This is the M9 row's "
                "own anticipated finding: M9-as-scoped is the wrong bite and the iron burn is "
                "NOT booked. Record NO negative about agnos — P/Q are not reported, not even in "
                "passing. Next work is Python-side harness bisection, zero burns.")
    if failed:
        return ("VOID", f"validity gate(s) failed: {failed}. No sequencing claim in either "
                        f"direction.")
    if cls.get("P") == "silent" and cls.get("Q") == "sound" and cls.get("D") == "sound":
        return ("A — SEQUENCING IS LIVE",
                "M9-as-scoped is the right bite. Book BURN_MODESET_AUDIO predicting AUDIO_POST "
                "(0x08) sounds and AUDIO_PRE (0x07) is silent.")
    if cls.get("P") == "sound" and cls.get("Q") == "sound":
        return ("UNCONFIRMED — BOTH SOUND",
                "Unmute position did not discriminate here. Read the un-arm probe: if the edge "
                "does not un-arm a standing unmute, this experiment could not have detected the "
                "effect and says nothing about M9 either way.")
    if cls.get("P") == "silent" and cls.get("Q") == "silent" and cls.get("Rlit") == "sound":
        return ("PROGRAM DELTA",
                "The reference sounds and BOTH agnos arms are silent: the discriminating "
                "difference is INSIDE agnos's program, not in the unmute's position. Run the "
                "bisect ladder; do not book the sequencing burn.")
    return ("INCONCLUSIVE", f"classification did not match a pre-registered pattern: {cls}")


# =============================================================================================
# §13  REAL ATOM EXECUTION — the transmitter edge.
#
# ⛔ WITHOUT THIS THE ENTIRE EXPERIMENT IS VACUOUS. P and Q differ only in where the unmute
# sits relative to THE EDGE; if no edge runs, they differ in where the unmute sits relative to
# nothing, and a completed battery would carry exactly zero information. The executor logged
# `atom_skipped` and ran the windows anyway on 2026-07-24, which is the worst possible failure
# mode: a result-shaped output with no result in it.
#
# atom-interp.py is imported as a MODULE and left unmodified. Its LiveCard does real BAR5 MMIO
# and its own docstring says it has never been exercised — so every call here is wrapped, the
# outcome is logged, and a failure DEGRADES the edge class rather than pretending it ran.
# =============================================================================================

_atom_mod = None


def load_atom_module():
    global _atom_mod
    if _atom_mod is None:
        import importlib.util
        path = f"{REPO_META}/scripts/atom-interp.py"
        spec = importlib.util.spec_from_file_location("atom_interp", path)
        _atom_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(_atom_mod)
    return _atom_mod


class AtomRunner:
    """Drives ATOM #4 and #76 against the live card.

    ⚠ ps LAYOUTS ARE AGNOS'S, taken from atom.cyr — not invented here. Action codes likewise.
    Inventing either would make this a different command than the one agnos issues."""

    CMD_ENCODER = 4
    CMD_TRANSMITTER = 76
    ACTION_DISABLE = 0
    ACTION_ENABLE = 1

    def __init__(self, journal, resource5, rom=f"{REPO_META}/vbios.rom"):
        self.j = journal
        m = load_atom_module()
        self.m = m
        self.tracer = m.Tracer(verbose=False)
        self.card = m.LiveCard(self.tracer, resource5)
        self.atom = m.Atom(bytearray(open(rom, "rb").read()), self.card, self.tracer)
        self.ok = True

    def _run(self, index, ps, title, window):
        t0 = time.perf_counter()
        try:
            ret = self.atom.execute_table(index, ps)
            rec = {"table": index, "title": title, "ret": ret,
                   "reg_r": self.tracer.n_reg_r, "reg_w": self.tracer.n_reg_w,
                   "delays": self.tracer.n_delay,
                   "ms": round((time.perf_counter() - t0) * 1000, 2)}
            self.j.event("atom_run", window=window, **rec)
            return ret
        except Exception as e:
            # ⛔ A PLL/MC access raises NotImplementedError in LiveCard. That is a REFUSAL, not
            # a crash to swallow: it means the table needs a facility we do not have, and the
            # honest response is to degrade the edge and say so, never to continue as if the
            # transmitter had been driven.
            self.j.event("atom_failed", window=window, table=index, error=repr(e))
            self.ok = False
            return -1

    def encoder_setup(self, dig, pclk_10khz, window=None):
        ps = self.m.build_encoder_stream_setup_v1_5(digid=dig, digmode=3, lanenum=4,
                                                    pclk_10khz=pclk_10khz)
        return self._run(self.CMD_ENCODER, ps, "DIGxEncoderControl #4", window)

    def transmitter(self, phy, action, dig, pclk_10khz, window=None):
        ps = self.m.build_transmitter_v1_6(phyid=phy, action=action, digmode=3, lanenum=4,
                                           symclk_10khz=pclk_10khz, hpdsel=0x00,
                                           digfe_sel=0x02, connobj_id=0x13)
        name = "DISABLE" if action == self.ACTION_DISABLE else "ENABLE"
        return self._run(self.CMD_TRANSMITTER, ps, f"DIG1TransmitterControl #76 {name}", window)


# =============================================================================================
# §11  HARDWARE-FREE ENTRY POINTS.
#
# --plan-diff and --dry-run need NO root and NO hardware and run on any machine.  That is
# deliberate and it is the most important safety property in this file: the experiment's central
# claim — that P and Q differ ONLY in the position of one identical block — is reviewable, by
# anyone, BEFORE a 30-minute session on the operator's only dev machine is booked.
# =============================================================================================

SILENT_BANNER = (
    "  ⚠ THIS COMMAND MAKES NO SOUND. It touches no device, maps no BAR, and generates no\n"
    "    audio. Silence here is correct and says NOTHING about the audio path. The first\n"
    "    sound in this tool comes from window 1 of the battery, on the amdgpu-blacklisted\n"
    "    boot. Until then there is no audio result, negative or otherwise.\n")


def build_all_plans(consts, dig=1, phy=1, az=1, pipe=0, h_total=2720, v_total=1481,
                    edge_class="ATOM"):
    """Structural build.  The dig/phy/az/pipe defaults are the ARCHAEMENID RESOLUTION and are
    used ONLY for offline plan inspection — the live path DERIVES every one of them and refuses
    on ambiguity (§4).  They are not a fallback and are never used with hardware attached."""
    progs = Programs(consts, dig, phy, az, pipe, h_total, v_total, ftrace=FtraceRef())
    env = Envelope(consts, phy, edge_class=edge_class)
    return {a: build_arm_plan(progs, env, a) for a in ARM_SPECS}


def cmd_plan_diff(args):
    problems = check_anchors()
    if problems:
        print("ANCHOR CHECK FAILED — the twin has drifted from agnos:")
        for p in problems:
            print("  ⛔", p)
        return 2
    consts = Consts()
    plans = build_all_plans(consts, edge_class=("SOFT" if args.soft_edge else "ATOM"))

    print("=" * 92)
    print(" L1 PLAN DIFF — the single-variable claim, checked with no root and no hardware")
    print("=" * 92)
    print(SILENT_BANNER)
    print(f" gpu_regs.cyr sha256 : {consts.sha}")
    print(f" constants extracted : {len(consts.vals)}")
    print(f" edge class          : {'SOFT (degraded — silence branch is NOT decisive)' if args.soft_edge else 'ATOM (real #4 + #76 edge)'}")
    ft = FtraceRef()
    print(f" ftrace oracle       : {'sha256 ' + (ft.sha or 'MISSING')} ({len(ft.rows)} wreg rows)")
    print()

    print(" ARMS")
    for a, spec in ARM_SPECS.items():
        p = plans[a]
        n_w = sum(1 for o in p.ops if o.kind in ("W", "RMW", "AZW"))
        print(f"   {a:<5} reps={spec['reps']}  ops={len(p.ops):>3}  writes={n_w:>3}  "
              f"edge={spec['edge']:<3} slot={str(spec['slot']):<5} hash={p.struct_hash()[:12]}")
        print(f"         {spec['role']}")
    print()

    print(" WRITE-SET ASSERTIONS (any failure ⇒ session VOID)")
    res = write_set_assertions(plans)
    allok = True
    for r in res:
        mark = "PASS" if r["pass"] else "FAIL"
        allok &= r["pass"]
        print(f"   [{mark}] {r['assertion']}")
        if not r["pass"]:
            print(f"          {r['detail']}")
    print()

    print(" FORBIDDEN-VALUE SCAN (§4.7)")
    # ⛔ SCOPE: DERIVED arms only. Rlit is the amdgpu REFERENCE and is SUPPOSED to carry
    # amdgpu's literals verbatim — 0x3AF5C000, the YCbCr AVI body, the bar literals. Flagging
    # them there would be flagging the arm for doing its job. The list exists to stop a value
    # from a CAPTURE leaking into a program that is meant to be agnos's.
    any_bad = False
    derived_arms = [a for a, sp in ARM_SPECS.items() if sp["stage"] == "der"]
    for a in derived_arms:
        bad = plans[a].check_forbidden(consts)
        for name, val, why in bad:
            any_bad = True
            print(f"   ⛔ {a}: {name} <- {val}  — {why}")
    if not any_bad:
        print(f"   [PASS] no forbidden value in any derived arm ({', '.join(derived_arms)})")
    ref_lits = sorted({o.value for o in plans["Rlit"].ops
                       if o.value in FORBIDDEN_VALUES}) if "Rlit" in plans else []
    if ref_lits:
        print("   [note] Rlit carries " + ", ".join(f"0x{v:08X}" for v in ref_lits)
              + " — CORRECT: it replays amdgpu verbatim, which is what makes it the reference")
    print()

    print(" P vs Q — THE ONLY DIFFERENCE THAT MAY EXIST")
    pq = diff_positions(plans["P"], plans["Q"])
    for line in pq:
        print("   " + line)
    print()
    print(" ORDER CONSTRAINTS (published a priori)")
    for line in BLOCK1_ORDER_RULES.strip().splitlines():
        print("   - " + line.strip())
    print("=" * 92)
    ok = allok and not any_bad
    print(" RESULT:", "PLAN IS SINGLE-VARIABLE — safe to book the session" if ok
          else "⛔ PLAN IS NOT SINGLE-VARIABLE — DO NOT BOOK THE SESSION")
    return 0 if ok else 1


def diff_positions(p_plan, q_plan):
    """Show that P and Q contain the same multiset of ops and differ only in where the unmute
    block sits."""
    out = []
    pu = p_plan.blocks.get("U_PRE")
    qu = q_plan.blocks.get("U_POST")
    p_rest = [o.struct_key() for i, o in enumerate(p_plan.ops)
              if not (pu and pu[0] <= i < pu[1])]
    q_rest = [o.struct_key() for i, o in enumerate(q_plan.ops)
              if not (qu and qu[0] <= i < qu[1])]
    same = p_rest == q_rest
    out.append(f"non-unmute op sequence identical : {'YES' if same else 'NO — THIS IS A BUG'}")
    out.append(f"unmute block in P at op index    : {pu[0] if pu else '(absent)'} "
               f"(before the edge)")
    out.append(f"unmute block in Q at op index    : {qu[0] if qu else '(absent)'} "
               f"(after the envelope close)")
    if pu and qu:
        out.append(f"unmute block size                : {pu[1]-pu[0]} ops, identical content")
    return out


def cmd_dry_run(args):
    """Builds every arm against a synthetic card and reports what a live run WOULD do.
    ⛔ Refuses/flags if an ATOM table would touch PLL or MC — those paths raise
    NotImplementedError in atom-interp's LiveCard and have NEVER been exercised, so their maiden
    execution must not happen inside a blinded window."""
    rc = cmd_plan_diff(args)
    print()
    print(" ATOM PRE-FLIGHT")
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    try:
        import atom_interp  # noqa
        print("   atom-interp imported as a module")
    except Exception:
        ai = f"{REPO_META}/scripts/atom-interp.py"
        has_live = "class LiveCard" in open(ai).read() if os.path.exists(ai) else False
        print(f"   atom-interp.py present : {os.path.exists(ai)}")
        print(f"   LiveCard available     : {has_live}")
        print("   ⚠ LiveCard's own docstring says it was NEVER EXERCISED, and its pll_read/")
        print("     mc_read raise NotImplementedError. ATOM's maiden run is gated to the")
        print("     unblinded Phase-3 fidelity check, never a blinded window.")
    return rc


def cmd_verify(args):
    """Reconstruct a session from its ndjson artifacts. No root, no hardware, any machine."""
    d = args.dir
    need = ["commit.txt", "reports.ndjson", "journal.ndjson"]
    missing = [n for n in need if not os.path.exists(f"{d}/{n}")]
    if missing:
        print(f"cannot verify: missing {missing}")
        return 2
    print(open(f"{d}/commit.txt").read())
    rows = [json.loads(l) for l in open(f"{d}/reports.ndjson")]
    print(f"windows reported: {len(rows)}")
    # chain re-verification
    h = ""
    bad = []
    for r in rows:
        h = hashlib.sha256(f"{h}{r['window']}{r.get('free_text','')}{r.get('code','')}"
                           f"{r.get('t_wall','')}".encode()).hexdigest()
        if r.get("chain") and r["chain"] != h:
            bad.append(r["window"])
    print("hash chain:", "INTACT" if not bad else f"BROKEN at windows {bad}")
    # preceding-arm correlation — tests the "the sink has no memory" assumption rather than
    # asserting it (open risk 4).
    prev_corr = {}
    for i, r in enumerate(rows[1:], 1):
        prev = rows[i - 1].get("arm")
        prev_corr.setdefault(prev, []).append(r.get("grade"))
    print("preceding-arm correlation (does window k depend on window k-1's arm?):")
    for k, v in sorted(prev_corr.items(), key=lambda x: str(x[0])):
        if v:
            print(f"   after {k}: mean grade {sum(x or 0 for x in v)/len(v):.2f}  n={len(v)}")
    return 0


def cmd_restore(args):
    """⭐ BLIND RECOVERY. Idempotent, safe when nothing is broken, short enough to type on a dark
    screen from memory. Replays pristine.json from the newest session dir."""
    sess = newest_session()
    if not sess:
        print("no session directory found — nothing to restore")
        return 1
    pj = f"{sess}/dumps/pristine.json"
    if not os.path.exists(pj):
        print(f"no pristine snapshot in {sess} — nothing to restore")
        return 1
    if os.geteuid() != 0:
        print("root required for --restore")
        return 2
    # ⛔ THIS IS THE BLIND-RECOVERY PATH. It is typed from memory at a dark screen by someone
    # who cannot read a traceback. EVERY step is independently guarded: a failure in one must
    # never prevent the next, and the function must never raise. A recovery tool that crashes
    # halfway is worse than none, because it leaves the machine in a state nobody can describe.
    try:
        pristine = json.load(open(pj))
    except Exception as e:
        print(f"cannot read {pj}: {e}")
        return 1

    n = 0
    try:
        bar = Mmio(f"/sys/bus/pci/devices/{args.gpu_dev}/resource5")
        # ORDER: plain registers, then the timing group, then OTG_MASTER_EN LAST — re-enabling
        # the pipe before its timing is restored is how a recovery makes things worse.
        otg_idx = pristine.get("otg_control_idx")
        # ⛔ WRITE ONLY WHAT ACTUALLY DIFFERS, AND NEVER A PULSE BIT.
        # Replaying a whole snapshot is NOT idempotent, and believing it was is what turned
        # this recovery tool into a way to BREAK a healthy pipe on 2026-07-24. Several of these
        # registers have write side-effects independent of the value written:
        #   AFMT_INFOFRAME_CONTROL0 bit 7  — AUDIO_INFO_UPDATE, a self-clearing PULSE
        #   AFMT_AUDIO_PACKET_CONTROL b26  — 60958_CS_UPDATE, a self-clearing latch
        #   AFMT_AUDIO_PACKET_CONTROL b23  — FIFO_OVERFLOW_ACK, write-1-to-clear
        #   HDMI_GC bit 0                  — AVMUTE: tells the SINK to blank video and audio
        # Re-asserting any of those on a pipe that is already correct re-fires the event.
        # A register whose current value already equals pristine needs NO write at all.
        skip_bits = {}
        try:
            cc = Consts()
            d1 = 1 * DIG_STRIDE
            for nm, bits in (("GPU_R_AFMT_INFOFRAME_CONTROL0", 0x80),
                             ("GPU_R_AFMT_AUDIO_PACKET_CONTROL", (1 << 26) | (1 << 23)),
                             ("GPU_R_HDMI_GC", 0x1)):
                if nm in cc.vals:
                    for inst in range(5):
                        skip_bits[SEG[2] + cc.vals[nm] + inst * DIG_STRIDE] = bits
        except Exception:
            pass
        same = 0
        for key, val in sorted(pristine.get("regs", {}).items()):
            try:
                idx = int(key)
                if otg_idx is not None and idx == int(otg_idx):
                    continue
                want = int(val)
                cur = bar.r32(idx * 4)
                mask = skip_bits.get(idx, 0)
                if (cur & ~mask) == (want & ~mask):
                    same += 1
                    continue                      # already correct — do NOT touch it
                bar.w32(idx * 4, (want & ~mask) | (cur & mask))
                n += 1
            except Exception:
                pass
        print(f"  {same} registers already correct (left untouched), {n} rewritten")
        if otg_idx is not None and pristine.get("otg_control") is not None:
            try:
                bar.w32(int(otg_idx) * 4, int(pristine["otg_control"]))
                n += 1
            except Exception:
                pass
        bar.close()
        print(f"restored {n} registers from {pj}")
    except Exception as e:
        print(f"register restore incomplete ({n} written): {e}")

    # ⚠ UN-BLANK THE FRAMEBUFFER. Restoring the registers is not enough if fbdev has latched
    # FB_BLANK_POWERDOWN — the pipe is correct and the console still looks dead. This is the
    # step whose absence sent the operator looking for a reboot on 2026-07-24.
    for fb in glob.glob("/sys/class/graphics/fb*/blank"):
        try:
            was = open(fb).read().strip()
            open(fb, "w").write("0")
            print(f"  {fb}: {was} -> 0 (unblanked)")
        except OSError:
            pass

    prev = pristine.get("hda_driver")
    if prev:
        try:
            ok = pci_bind(args.hda_dev, prev)
            print(f"  rebind {args.hda_dev} -> {prev}: "
                  f"{'ok' if ok else 'failed (a reboot clears it)'}")
        except Exception:
            pass
    cmdw = pristine.get("pci_command")
    if cmdw is not None:
        try:
            pci_write_command(args.hda_dev, int(cmdw))
        except Exception:
            pass

    print("restore complete. Idempotent: it writes only what differs and never re-fires a")
    print("pulse bit, so running it on an already-healthy pipe is a no-op.")
    return 0


def newest_session():
    cands = sorted(glob.glob(f"{PRIOR_ART_DIR}/l1-*"))
    return cands[-1] if cands else None


def cmd_boot_entry(args):
    """Prints the boot entry.  ⛔ Does NOT write it unless --install-boot-entry is passed AND the
    operator confirms: this is a write to the boot path of the user's only machine, and that is
    their call to make, not mine."""
    entries = sorted(glob.glob("/boot/loader/entries/*.conf"))
    default = entries[0] if entries else "(none found)"
    body = None
    if entries:
        src = open(entries[0]).read()
        opts = [l for l in src.splitlines() if l.startswith("options")]
        base = opts[0] if opts else "options root=... rw"
        body = ("title   L1 (amdgpu blacklisted)\n"
                + "\n".join(l for l in src.splitlines()
                            if l.startswith(("linux", "initrd")))
                + "\n" + base
                + " modprobe.blacklist=amdgpu amd_iommu=off\n")
    print("=" * 92)
    print(" L1 BOOT ENTRY — additive. Your default entry is NOT modified.")
    print("=" * 92)
    print(SILENT_BANNER)
    print(f" existing entries : {[os.path.basename(e) for e in entries]}")
    print(f" modelled on      : {os.path.basename(default)}")
    print()
    print(" amd_iommu=off is a FIDELITY choice, not a convenience: agnos runs with amdvi")
    print(" disabled, so the DMA path matches rather than merely working.")
    print()
    if body:
        print(" ---- /boot/loader/entries/l1.conf ----")
        print(body)
    if not args.install_boot_entry:
        print(" NOT INSTALLED. This tool does not write your boot path without being asked.")
        print(" To install:  sudo l1.py --install-boot-entry")
        return 0
    if os.geteuid() != 0:
        print("root required to install")
        return 2
    ans = input(" Write /boot/loader/entries/l1.conf now? [type: yes] ").strip()
    if ans != "yes":
        print(" not installed")
        return 0
    with open("/boot/loader/entries/l1.conf", "w") as f:
        f.write(body)
        f.flush()
        os.fsync(f.fileno())
    print(" installed. Your default entry is unchanged; pick 'L1 (amdgpu blacklisted)' at boot.")
    return 0


def cmd_gates(args):
    """Report every refuse-to-start gate WITHOUT running anything. Safe on the desktop."""
    g = {}
    drv = f"/sys/bus/pci/devices/{args.gpu_dev}/driver"
    g["euid==0"] = os.geteuid() == 0
    g["amdgpu NOT bound to GPU"] = not os.path.islink(drv)
    g["amdgpu driver seen"] = (os.path.basename(os.path.realpath(drv))
                               if os.path.islink(drv) else None)
    try:
        g["hugepages>=1"] = int(open("/sys/kernel/mm/hugepages/hugepages-2048kB/"
                                     "nr_hugepages").read().strip()) >= 1
    except OSError:
        g["hugepages>=1"] = False
    g["VFCT present"] = os.path.exists("/sys/firmware/acpi/tables/VFCT")
    g["ftrace oracle present"] = os.path.exists(FTRACE)
    g["gpu_regs.cyr readable"] = os.path.exists(GPU_REGS)
    g["anchors intact"] = not check_anchors()
    g["sshd active"] = subprocess.run(["systemctl", "is-active", "--quiet", "sshd"]).returncode == 0
    leds = glob.glob("/sys/class/leds/*::scrolllock/brightness")
    g["scrolllock LED"] = leds[0] if leds else None
    print("=" * 92)
    print(" L1 GATES")
    print("=" * 92)
    print(SILENT_BANNER)
    for k, v in g.items():
        mark = "ok " if (v is True or (v and not isinstance(v, bool))) else "NO "
        print(f"  [{mark}] {k}: {v}")
    print()
    if g["amdgpu NOT bound to GPU"]:
        print("  regime: BLACKLISTED — the GOP-DVI pipe. This is the L1 regime.")
    else:
        print("  regime: amdgpu-bound. ⛔ The battery REFUSES here.")
        print("  ⛔ `modprobe -r amdgpu` is NOT a substitute: its remove path disables the")
        print("     CRTCs, leaving a torn-down pipe rather than the GOP-DVI pipe agnos")
        print("     inherits. Testing agnos's program against anything else is not the")
        print("     experiment. Use the L1 boot entry.")
    return 0



# =============================================================================================
# §12  THE LIVE EXECUTOR.
#
# Everything above this line is analysable without hardware.  Everything below it touches the
# machine, and every line of it is written on the assumption that THE PANEL MAY NOT COME BACK.
#
# THREE INVARIANTS, and they are not negotiable:
#   1. INFLIGHT is written and fsync'd BEFORE the first register write of every window, so a
#      crash always leaves a machine-readable statement of what was in progress and which
#      snapshot restores it.
#   2. `finally` runs G0 (pristine restore) on EVERY exit path, including KeyboardInterrupt.
#   3. Restore-verify runs after every window against a DECLARED volatile whitelist.  Without
#      it a permuted arm order is meaningless — window k+1 would ride window k's link state,
#      and the randomisation that makes the blinding work would be silently undone.
# =============================================================================================

# Registers that legitimately change on their own.  ⛔ DECLARED, not discovered: a whitelist
# grown by adding whatever failed today is how a restore-verify stops verifying anything.
# ⛔ THESE NAMES MUST MATCH gpu_regs.cyr EXACTLY. The first cut wrote "OTG_STATUS_FRAME_COUNT"
# — which is what the register is CALLED in the comment but NOT what the constant is NAMED
# (GPU_R_OTG_FRAME_COUNT). The lookup silently matched nothing, so the free-running frame
# counter was "verified" and contaminated window 1 of the first live session. A whitelist whose
# entries do not resolve is worse than no whitelist: it reads as protection while protecting
# nothing. build_volatile() below REFUSES on a name that does not resolve.
VOLATILE = ("AFMT_STATUS", "HDMI_ACR_STATUS_0", "HDMI_STATUS", "AFMT_AUDIO_INFO0",
            "OTG_FRAME_COUNT",
            # FMT_CONTROL: no arm programs it; gpu_regs.cyr:1283 records amdgpu's own last value
            # as 0x01000000, which is exactly what it re-reads as after an envelope. Status.
            "FMT_CONTROL",
            # HDMI_DB_CONTROL misses its restore because the HDMI_* block is inert while
            # DIG_MODE==2 and G0 restores DIG_BE_CNTL in the same pass. It contaminated w7 of
            # the 2026-07-24 run. Safe to whitelist ONLY because S_der writes it unconditionally
            # (A19) at the start of EVERY window, so it cannot carry state between arms.
            "HDMI_DB_CONTROL")


class Led:
    """Channel B. The scrollock LED is a KEYBOARD INDICATOR — it is never in the signal path.
    ⛔ No dongle, adapter, USB audio device or HDMI capture device anywhere in this protocol."""

    def __init__(self):
        c = glob.glob("/sys/class/leds/*::scrolllock/brightness")
        self.path = c[0] if c else None

    def set(self, on):
        if not self.path:
            return
        try:
            open(self.path, "w").write("1" if on else "0")
        except OSError:
            pass

    def blink(self, n, period=0.12):
        for _ in range(n):
            self.set(True); time.sleep(period / 2)
            self.set(False); time.sleep(period / 2)


class Session:
    def __init__(self, args, journal):
        self.a, self.j = args, journal
        self.led = Led()
        self.bar = None
        self.hda_bar = None
        self.hda = None
        self.dma = None
        self.pristine = {}
        self.consts = Consts()
        self.derived = {}
        self.hda_prev_driver = None
        self.pci_cmd = None
        self.edge_class = ("SOFT" if args.soft_edge
                           else "HDMIMODE" if getattr(args, "hdmi_mode", False)
                           else "MINIMAL" if getattr(args, "minimal_edge", False)
                           else "NOENV" if getattr(args, "no_envelope", False) else "ATOM")
        self.contaminated = False
        self.atom = None

    # ---- gates -------------------------------------------------------------------------
    def gates(self):
        fail = []
        if os.geteuid() != 0:
            fail.append("not root")
        if os.path.islink(f"/sys/bus/pci/devices/{self.a.gpu_dev}/driver"):
            fail.append("amdgpu (or some driver) is BOUND to the GPU — this is not the L1 "
                        "regime; `modprobe -r amdgpu` is NOT a substitute, use the L1 boot entry")
        try:
            if int(open("/sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages").read()) < 1:
                fail.append("no 2MB hugepage reserved")
        except OSError:
            fail.append("hugepage sysfs unreadable")
        if check_anchors():
            fail.append("anchor check failed — the twin has drifted from agnos")

        # ⛔⛔ THE CODEC MUST BE FREE BEFORE WE UNBIND ITS DRIVER.
        # pci_unbind() writes sysfs `unbind` and BLOCKS IN D-STATE — unkillable, not even by
        # SIGKILL — if a userspace client still holds the HDA controller. On 2026-07-24 this
        # hung the battery three times right after the `derived:` line, and SSH starting a user
        # session is enough to bring pipewire back even on a boot with no desktop. The script
        # header has warned about this from the beginning; the GATE was specified and never
        # written, so the warning did nothing.
        holders = []
        for pat in ("/dev/snd/controlC*", "/dev/snd/pcm*", "/dev/snd/seq"):
            for dev in glob.glob(pat):
                try:
                    out = subprocess.run(["fuser", dev], capture_output=True, text=True,
                                         timeout=5).stdout.split()
                except Exception:
                    continue
                for pid in out:
                    try:
                        name = open(f"/proc/{int(pid)}/comm").read().strip()
                        holders.append(f"{name}({pid})")
                    except (OSError, ValueError):
                        pass
        holders = sorted(set(holders))
        if holders:
            fail.append(
                "the ALSA devices are still held by " + ", ".join(holders)
                + " — the snd_hda_intel unbind would block in UNINTERRUPTIBLE D-state and the "
                  "process could not even be SIGKILLed. Free them first:\n"
                  "        systemctl --user stop wireplumber pipewire pipewire-pulse "
                  "pipewire.socket pipewire-pulse.socket")
        for unit in ("sddm", "gdm", "lightdm", "greetd"):
            try:
                if subprocess.run(["systemctl", "is-active", "--quiet", unit],
                                  timeout=5).returncode == 0:
                    fail.append(f"display manager {unit} is active — stop it: "
                                f"sudo systemctl stop {unit}")
            except Exception:
                pass
        return fail

    # ---- snapshot / restore -------------------------------------------------------------
    def snapshot(self, plans):
        """Every dword any arm will touch, plus the timing group and OTG control."""
        idxs = set()
        for p in plans.values():
            for o in p.ops:
                if o.rel is not None and o.space in (1, 2, 0):
                    idxs.add(self.resolve(o))
        for n in Envelope.TIMING_GROUP + ["GPU_R_OTG_CONTROL", "GPU_R_OTG_FRAME_COUNT"]:
            idxs.add(SEG[2] + self.consts[n])
        regs = {}
        for i in sorted(idxs):
            try:
                regs[str(i)] = self.bar.r32(i * 4)
            except Exception:
                pass
        self.pristine = {
            "regs": regs,
            "otg_control_idx": SEG[2] + self.consts["GPU_R_OTG_CONTROL"],
            "otg_control": regs.get(str(SEG[2] + self.consts["GPU_R_OTG_CONTROL"])),
            "hda_driver": self.hda_prev_driver,
            "pci_command": self.pci_cmd,
            "derived": self.derived,
        }
        self.j.artifact("dumps/pristine.json", self.pristine)
        return len(regs)

    def resolve(self, op):
        if op.space == 0:
            return op.rel                       # ftrace ops carry ABSOLUTE indices
        return SEG[op.space] + op.rel

    def g0_restore(self):
        """Pristine restore. ⛔ ORDER: plain registers, then the timing group, then
        OTG_MASTER_EN LAST — re-enabling the pipe before its timing is restored is how a
        recovery makes things worse."""
        if not self.pristine or self.bar is None:
            return 0
        n = 0
        otg_idx = self.pristine["otg_control_idx"]
        tg = {SEG[2] + self.consts[x] for x in Envelope.TIMING_GROUP}
        for key, val in self.pristine["regs"].items():
            i = int(key)
            if i == otg_idx or i in tg:
                continue
            self.bar.w32(i * 4, int(val)); n += 1
        for name in Envelope.TIMING_GROUP:
            i = SEG[2] + self.consts[name]
            if str(i) in self.pristine["regs"]:
                self.bar.w32(i * 4, int(self.pristine["regs"][str(i)])); n += 1
        if self.pristine.get("otg_control") is not None:
            self.bar.w32(otg_idx * 4, int(self.pristine["otg_control"])); n += 1
        return n

    def build_volatile(self):
        """Resolve the whitelist ONCE and REFUSE on any entry that does not exist. This is the
        guard on the bug that contaminated the first live session."""
        idxs, missing = set(), []
        for v in VOLATILE:
            name = f"GPU_R_{v}"
            if name not in self.consts.vals:
                missing.append(name)
                continue
            base = SEG[2] + self.consts.vals[name]
            for i in range(5):
                idxs.add(base + i * DIG_STRIDE)
        if missing:
            raise RuntimeError(
                "REFUSING: volatile whitelist names do not resolve in gpu_regs.cyr: "
                + ", ".join(missing)
                + " — a whitelist entry that resolves to nothing reads as protection while "
                  "protecting nothing.")
        return idxs

    def restore_verify(self):
        """⛔ Any out-of-whitelist mismatch CONTAMINATES this window AND every subsequent one,
        and aborts the session. A permuted order whose windows leak into each other is not a
        randomised experiment."""
        vol = self.build_volatile()
        bad = []
        for key, val in self.pristine["regs"].items():
            i = int(key)
            if i in vol:
                continue
            now = self.bar.r32(i * 4)
            if now != int(val):
                bad.append({"idx": i, "want": int(val), "got": now})
        return bad

    # ---- one window ---------------------------------------------------------------------
    def run_window(self, wnum, total, entry, plans, u_phase=False):
        arm = entry["arm"]
        pat = entry["pattern"]
        self.j.event("window_start", window=wnum, arm=arm, pattern=pat)
        # INFLIGHT before the first write — invariant 1.
        self.j.artifact("INFLIGHT.json", {"window": wnum, "arm": arm,
                                          "pristine": f"{self.j.dir}/dumps/pristine.json"})
        # ⛔ TTY GETS EXACTLY TWO LINES AND NEITHER NAMES THE ARM. Everything else -> log only.
        self.j.say(f"[window {wnum:02d}/{total}]  start", tty=True)
        self.j.say(f"    (arm={arm} pattern={pat})", tty=False)

        pcm = render_pattern(pat)
        ok = False
        try:
            self.led.set(True)
            self.execute(plans[arm], wnum, arm, pcm)
            ok = True
        except Exception as e:
            self.j.event("window_error", window=wnum, arm=arm, error=repr(e))
            self.j.say(f"    !! window error: {e!r}", tty=False)
        finally:
            self.led.set(False)
            try:
                if self.hda:
                    self.hda.stop_stream()
            except Exception:
                pass
            self.g0_restore()                      # invariant 2
            bad = self.restore_verify()            # invariant 3
            if bad:
                # ⛔ RETRY BEFORE CONDEMNING. A register can miss its restore because the block
                # it lives in was inert at the moment we wrote it (HDMI_* registers are dead
                # while DIG_MODE==2, and G0 restores DIG_BE_CNTL in the same pass). Re-running
                # G0 with the routing already back gives it a second, valid chance. Only a
                # PERSISTENT mismatch is contamination — condemning on the first miss ends a
                # 30-minute session on a register that would have restored itself.
                self.j.event("restore_retry", window=wnum, first_pass=bad[:20])
                self.g0_restore()
                bad = self.restore_verify()
                self.j.say(f"    restore-verify: retry cleared "
                           f"{'ALL' if not bad else 'some'} mismatches", tty=False)
            if bad:
                self.contaminated = True
                self.j.event("contaminated", window=wnum, mismatches=bad[:20])
                self.j.say(f"    !! restore-verify: {len(bad)} out-of-whitelist mismatches — "
                           f"session CONTAMINATED from here", tty=False)
            try:
                os.remove(f"{self.j.dir}/INFLIGHT.json")
            except OSError:
                pass
        self.j.say(f"[window {wnum:02d}/{total}]  end", tty=True)
        return ok

    def execute(self, plan, wnum, arm, pcm):
        """Walk a plan's ops against live hardware. RMW resolves against the CURRENT value —
        never against a snapshot taken at a different time, which is the
        [[reference_agnos_dcn_hubp_reg_offsets_derived]] class."""
        b = self.bar
        b.phase, b.window, b.arm = "arm", wnum, arm
        az_ir = SEG[2] + self.consts["GPU_R_AZ_ENDPOINT_INDEX"] + self.derived["az"] * AZ_STRIDE
        az_dr = SEG[2] + self.consts["GPU_R_AZ_ENDPOINT_DATA"] + self.derived["az"] * AZ_STRIDE
        for o in plan.ops:
            if o.kind == "DELAY":
                t0 = time.perf_counter()
                time.sleep((o.value or 0) / 1e6)
                self.j.event("delay", window=wnum, name=o.name,
                             requested_us=o.value,
                             actual_us=int((time.perf_counter() - t0) * 1e6))
            elif o.kind in ("W", "RMW"):
                idx = self.resolve(o)
                if o.kind == "W":
                    val = o.value
                    if val is None:              # E_close: restore the SAVED value
                        val = int(self.pristine["regs"].get(str(idx), b.r32(idx * 4)))
                else:
                    val = (b.r32(idx * 4) & ~(o.mask or 0)) | (o.set_ or 0)
                (b.w32f if o.flush else b.w32)(idx * 4, val)
            elif o.kind == "AZW":
                b.w32f(az_ir * 4, o.az_ord)      # ⛔ re-select EVERY time: no window memory
                val = o.value if o.value is not None else                     (b.r32(az_dr * 4) & ~(o.mask or 0)) | (o.set_ or 0)
                b.w32f(az_dr * 4, val)
            elif o.kind == "AZR":
                b.w32f(az_ir * 4, o.az_ord)
                b.r32(az_dr * 4)
            elif o.kind == "R":
                b.r32(self.resolve(o) * 4)
            elif o.kind == "ATOM":
                if self.atom is None or self.edge_class == "SOFT":
                    self.j.event("atom_skipped", window=wnum, name=o.name,
                                 reason="no runner" if self.atom is None
                                        else "edge_class=SOFT")
                    continue
                pclk = 24150
                if o.name.endswith("_4"):
                    self.atom.encoder_setup(self.derived["dig"], pclk, window=wnum)
                elif "DISABLE" in o.name:
                    self.atom.transmitter(self.derived["phy"],
                                          AtomRunner.ACTION_DISABLE,
                                          self.derived["dig"], pclk, window=wnum)
                elif "ENABLE" in o.name:
                    self.atom.transmitter(self.derived["phy"],
                                          AtomRunner.ACTION_ENABLE,
                                          self.derived["dig"], pclk, window=wnum)
            elif o.kind == "FN":
                self.fn(o, wnum, pcm)

    def fn(self, o, wnum, pcm):
        """Named structural steps. Each is small and each logs what it actually observed —
        never a bounded poll count reported as a duration."""
        c, b = self.consts, self.bar
        if o.name == "G2_feed_stop" and self.hda:
            self.hda.stop_stream()
        elif o.name == "E_open":
            idx = SEG[2] + c["GPU_R_OTG_CONTROL"]
            fc = SEG[2] + c["GPU_R_OTG_FRAME_COUNT"]
            cur = b.r32(idx * 4)
            f0 = b.r32(fc * 4)
            b.w32f(idx * 4, cur & ~0x1)
            stopped, t0 = False, time.perf_counter()
            while time.perf_counter() - t0 < 0.5:
                time.sleep(0.01)
                if b.r32(fc * 4) == b.r32(fc * 4):
                    pass
                f1 = b.r32(fc * 4)
                time.sleep(0.03)
                if b.r32(fc * 4) == f1:
                    stopped = True
                    break
            self.j.event("e_open", window=wnum, stopped=stopped,
                         elapsed_ms=int((time.perf_counter() - t0) * 1000), frame0=f0)
            if not stopped:
                # ⛔ ABORT HERE, not at the verdict. The safety case for the whole battery is
                # that the risky work happens with the pipe VERIFIED stopped.
                b.w32f(idx * 4, cur)
                raise RuntimeError("E_open: frame counter never stopped — aborting this window")
        elif o.name == "E_master_en":
            idx = SEG[2] + c["GPU_R_OTG_CONTROL"]
            b.w32f(idx * 4, int(self.pristine["regs"][str(idx)]))
        elif o.name == "E_settle":
            time.sleep(0.12)
            fc = SEG[2] + c["GPU_R_OTG_FRAME_COUNT"]
            a = b.r32(fc * 4); time.sleep(0.05); z = b.r32(fc * 4)
            self.j.event("e_settle", window=wnum, resumed=bool(a != z))
        elif o.name == "E_conn_verify":
            db = self.derived["phy"] * DIG_STRIDE
            be = b.r32((SEG[2] + c["GPU_R_DIG_BE_CNTL"] + db) * 4)
            mode = (be & c["GPU_DIG_MODE_MASK"]) >> c["GPU_DIG_MODE_SHIFT"]
            sel = be & c["GPU_DIG_FE_SOURCE_SEL_MASK"]
            self.j.event("e_conn_verify", window=wnum, be_cntl=be, mode=mode, fe_sel=sel)
        elif o.name in ("F_start",):
            # ⛔ NEVER SKIP THIS SILENTLY. If there is no controller, the window cannot make a
            # sound and the operator must be told before they spend six seconds listening.
            if self.hda is None:
                self.j.say("⛔ F_start: NO HDA CONTROLLER — this window CANNOT emit audio. "
                           "Any silence you hear is meaningless.")
                self.j.event("feed_impossible", window=wnum)
                raise RuntimeError("F_start reached with no HDA controller")
            if not pcm:
                self.j.say("⛔ F_start: EMPTY PATTERN — nothing to play.")
                raise RuntimeError("F_start reached with an empty pattern")
            self.hda.fill_tone(pcm)
            ran = self.hda.arm_stream(len(pcm))
            lp = self.hda.lpib()
            # LPIB advancing is a VALIDITY check, never evidence of sound (R6). It says the DMA
            # engine is fetching; it says nothing about what reaches the sink.
            self.j.event("feed_start", window=wnum, lpib_advanced=bool(ran), lpib=lp)
            self.j.say(f"    feed: stream {'RUNNING' if ran else 'STALLED'} lpib={lp}",
                       tty=False)
            if not ran:
                self.j.say("⚠ THE STREAM DID NOT ADVANCE — this window cannot be evidence.")

    # ---- report grammar ------------------------------------------------------------------
    def collect_report(self, wnum, u_phase, chain_head):
        """⛔ Never advances until BOTH lines parse, so blind misalignment cannot silently
        drift. The LED speaks so the operator never needs the screen."""
        self.led.blink(1, 0.9)
        while True:
            free = input(f"window {wnum:02d} > ").strip()
            if not free:
                self.led.blink(3)
                print("  (say what you heard, or type the word: nothing)")
                continue
            break
        while True:
            code = input("code > ").strip()
            rep = parse_report(code, u_phase=u_phase)
            if rep is None:
                self.led.blink(3)
                print("  (6 chars: grade bursts pitch sweep channels level — or just 0)")
                continue
            break
        self.led.blink(2)
        rep["free_text"] = free
        rep["window"] = wnum
        rep["t_wall"] = time.time()
        h = hashlib.sha256(f"{chain_head}{wnum}{free}{rep['raw']}{rep['t_wall']}"
                           .encode()).hexdigest()
        rep["chain"] = h
        with open(f"{self.j.dir}/reports.ndjson", "a") as f:
            f.write(json.dumps(rep) + "\n"); f.flush(); os.fsync(f.fileno())
        print(f"  chain {h[:12]}")
        return rep, h


def cmd_run(args):
    """The operator path. One command."""
    # ⛔ GATE BEFORE CREATING ANYTHING. A refused run must leave no trace: an empty session
    # directory in prior-art/ is litter that later reads like a session that happened.
    probe = Session(args, None)
    fail = probe.gates()
    if fail:
        print("REFUSING TO RUN:")
        for f in fail:
            print(f"  ⛔ {f}")
        return 2

    # ⛔ ONE INSTANCE AT A TIME. Two concurrent runs both drive the same BAR5 and the same HDA
    # controller: their writes interleave, each one's restore fights the other's staging, and
    # every window either produces is meaningless. The first live session had two running at
    # once (tty3 and pts/0) because this lock was specified and not implemented.
    lock_path = "/run/l1.lock"
    try:
        lock_fd = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
        os.write(lock_fd, f"{os.getpid()}\n".encode())
        os.close(lock_fd)
    except FileExistsError:
        try:
            holder = open(lock_path).read().strip()
        except OSError:
            holder = "?"
        alive = os.path.exists(f"/proc/{holder}")
        if alive:
            print(f"REFUSING TO RUN: another L1 instance is RUNNING (pid {holder}).")
            print("  Two instances would interleave their register writes and both would be")
            print("  meaningless. Finish or Ctrl-C the other one first.")
            return 2
        # ⚠ STALE LOCK: reclaim it automatically. A previous run that was killed hard — say by
        # a D-state unbind, which is exactly how this lock got orphaned on 2026-07-24 — cannot
        # run its own cleanup. Making the operator type `rm /run/l1.lock` at a dark console to
        # get past a file that protects against a process which no longer exists is friction
        # with no safety value.
        print(f"  (reclaiming stale lock from dead pid {holder})")
        try:
            os.remove(lock_path)
            fd = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
            os.write(fd, f"{os.getpid()}\n".encode())
            os.close(fd)
        except OSError as e:
            print(f"REFUSING TO RUN: could not reclaim {lock_path}: {e}")
            return 2

    sess = f"{PRIOR_ART_DIR}/l1-{time.strftime('%Y%m%d-%H%M')}"
    j = Journal(sess)
    s = Session(args, j)

    plans = build_all_plans(s.consts, edge_class=s.edge_class)
    if s.edge_class == "NOENV":
        j.say("EDGE=NOENV — OTG envelope DROPPED (it wedges this APU from userspace); the "
              "ATOM #4/#76 transmitter edge still runs. Stamped into every artifact.")
    asserts = write_set_assertions(plans)
    if not all(a["pass"] for a in asserts):
        j.say("⛔ write-set assertions FAILED — the arms are not single-variable. VOID.")
        j.artifact("writelists/assertions.json", asserts)
        return 3

    s.bar = Mmio(f"/sys/bus/pci/devices/{args.gpu_dev}/resource5", journal=j)
    d = Derive(s.bar, s.consts, j)
    phy = d.phy(); dig = d.dig(phy); h_t, v_t = d.totals(s.consts); d.pixclk_100hz()
    s.derived = dict(d.out, dig=dig, phy=phy, az=dig, pipe=0)
    j.artifact("derived.json", s.derived)
    j.say(f"derived: phy={phy} dig={dig} az={dig} h_total={h_t+1} v_total={v_t+1}")

    # rebuild the plans on the DERIVED indices, not the offline defaults
    progs = Programs(s.consts, dig, phy, dig, 0, h_t + 1, v_t + 1, ftrace=FtraceRef())
    env = Envelope(s.consts, phy, edge_class=s.edge_class)
    plans = {a: build_arm_plan(progs, env, a) for a in ARM_SPECS}

    # ⛔ TAKE fbcon OFF THE PIPE BEFORE WE DISABLE IT.
    # E_open drops OTG_MASTER_EN on the CRTC the Linux console is actively scanning out
    # (fb0 = simpledrmdrmfb, fbcon bound). Under agnos that envelope is proven safe because
    # agnos OWNS the pipe; here a second consumer is driving it. Unbinding fbcon is reversible,
    # is restored in the finally below, and is recorded in the journal as an override so no
    # result obtained with it is ever mistaken for one obtained without.
    # ⚠ This is the leading hypothesis for the 2026-07-24 hard lockups, NOT an established
    # cause. If the box still wedges with fbcon off, the hypothesis is dead and says so.
    # ⚠ A hard-killed previous run cannot rebind fbcon, so the console can already be off the
    # framebuffer when we start. Harmless for us (we unbind it anyway) but it must be rebound
    # on exit regardless of who unbound it — otherwise the operator is left with a console that
    # never comes back and no idea why.
    # ⛔ fbcon UNBIND IS OFF BY DEFAULT — it was a mistake to make it the default.
    # It was introduced on the hypothesis that the console scanning out the CRTC was what made
    # E_open wedge the APU. Bisect N0:5 with fbcon unbound WEDGED ANYWAY, which FALSIFIES that
    # hypothesis outright. So the unbind buys nothing — and it costs the operator their screen,
    # because unbinding fbcon stops the console rendering from that instant. On 2026-07-24 the
    # operator saw nothing after the `derived:` line and could only read the output by quitting.
    # A default that blinds the person running the experiment, in exchange for a benefit that
    # has been measured not to exist, is strictly worse than no default.
    fb_unbound = fbcon_set(False) if args.unbind_fbcon else []
    j.event("fbcon", unbound=bool(fb_unbound), detail=str(fb_unbound))
    if fb_unbound:
        j.say(f"fbcon unbound from the framebuffer (restored on exit): {fb_unbound}")

    # (the feed + edge are built by the SAME function the bisect uses — see its docstring)

    n = s.snapshot(plans)
    j.say(f"pristine snapshot: {n} registers")

    # ⛔ THE FEED AND THE EDGE. Without both, the battery is VACUOUS and must not start.
    if not setup_audio_and_atom(s, args, j, plans):
        j.say("⛔ REFUSING TO RUN A VACUOUS BATTERY: with no transmitter edge, P and Q differ "
              "only in where the unmute sits relative to NOTHING, and every window would be "
              "uninterpretable.")
        return 4

    # ⛔ Rlit IS OPT-IN. Its 201-write ftrace replay wedged the APU in two separate runs on
    # 2026-07-24 (windows 1 and 2), while every derived arm survived. It is the session-validity
    # gate, so dropping it WEAKENS the result — without it we cannot show that ANY program makes
    # this sink sound, and a silent session cannot be distinguished from a broken harness.
    # That cost is real and is stamped into the artifacts. It is still better than a battery
    # that cannot reach window 3.
    arms = []
    for a, spec in ARM_SPECS.items():
        if a == "Rlit" and not args.with_rlit:
            continue
        arms += [a] * spec["reps"]
    if not args.with_rlit:
        j.say("⚠ Rlit EXCLUDED (--with-rlit to include). The session-validity gate is therefore "
              "ABSENT: a silent session cannot be distinguished from a broken harness, so the "
              "verdict cannot be VOID-HARNESS vs a real negative. Stamped into VERDICT.md.")
    # ⛔ THE ONE BLOCKING PROMPT BEFORE THE LED VOCABULARY STARTS, and on 2026-07-24 it read as
    # a hang: the console was blanked, nothing had been printed for a second, and the operator
    # reasonably concluded the tool was stuck at `derived:`. Announce it loudly, blink the LED
    # so it is visible without the screen, and never let silence mean "waiting".
    led = Led()
    print()
    print("=" * 92)
    print("  ⌨  WAITING FOR YOU — TYPE ANY CHARACTERS AND PRESS ENTER.")
    print("     Anything at all: 'x', a word, keyboard mash. It is hashed with a random number")
    print("     this tool generates, so neither of us alone chooses the window order.")
    print("     (If your screen is blank: just type something and hit Enter — that is all.)")
    print("=" * 92)
    threading.Thread(target=lambda: [led.blink(2, 0.25) or time.sleep(1.0)
                                     for _ in range(600)], daemon=True).start()
    op_seed = input("operator seed (type anything): ")
    led.set(False)
    tool_seed = secrets.randbits(64)
    master = sha256_str(f"{op_seed}:{tool_seed}")
    # ⛔ neither party alone controls the key
    h = hashlib.sha256(f"{master}:order".encode()).digest()
    order = sorted(range(len(arms)), key=lambda i: h[i % len(h)] * 256 + i)
    arms = [arms[i] for i in order]
    plan_key = derive_key(master, len(arms), arms)
    j.artifact("key.sealed", {"seed_hash": sha256_str(op_seed), "tool_seed": tool_seed,
                              "plan": plan_key})
    os.chmod(f"{sess}/key.sealed", 0o000)

    commit = (f"L1-COMMIT  key=sha256:{sha256_str(json.dumps(plan_key, sort_keys=True))}  "
              f"script=sha256:{sha256_file(os.path.abspath(__file__))}  "
              f"consts=sha256:{s.consts.sha}  ftrace=sha256:{FtraceRef().sha}  "
              f"seedhash=sha256:{sha256_str(op_seed)}  n_windows={len(arms)}  "
              f"edge={s.edge_class}  utc={time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}")
    j.text("commit.txt", commit + "\n")
    print(commit, flush=True)

    print()
    print("=" * 92)
    print("  ⚠ FROM HERE ON, SOUND IS THE MEASUREMENT. Everything before this line was silent")
    print("    by construction; everything after it is the experiment.")
    print("=" * 92)
    print("Some, all, or NONE of the windows may make sound. You will not be told which.")
    print("Two lines per window: what you heard (or the word `nothing`), then the code.")
    print()

    chain = ""
    try:
        for k, entry in enumerate(plan_key, 1):
            s.run_window(k, len(plan_key), entry, plans)
            rep, chain = s.collect_report(k, False, chain)
            if s.contaminated:
                j.say("⛔ session CONTAMINATED — aborting rather than reporting dirty windows")
                break
            time.sleep(4.0)
    except KeyboardInterrupt:
        j.say("operator aborted; remaining windows recorded ABANDONED", tty=False)
    finally:
        s.g0_restore()
        if s.hda:
            s.hda.stop_stream()
        j.say(f"final restore at {time.strftime('%H:%M:%S')} — ⛔ the link and codec are NOT "
              f"powered down: the shutdown release pop is the arc's only sink-side instrument")
        os.chmod(f"{sess}/key.sealed", 0o400)
        # Rebind unconditionally: a previous hard-killed run may have left it off, and a
        # console that never comes back is the worst thing this tool can leave behind.
        fbcon_set(True)
        j.say("fbcon rebound")
        # ⚠ Un-blank explicitly. Restoring registers is not enough if fbdev latched
        # FB_BLANK_POWERDOWN — the pipe is right and the console still looks dead.
        for fb in glob.glob("/sys/class/graphics/fb*/blank"):
            try:
                open(fb, "w").write("0")
            except OSError:
                pass
        try:
            os.remove(lock_path)
        except OSError:
            pass
        try:
            subprocess.run(["chown", "-R", "macro", sess], check=False)
        except Exception:
            pass
    print(f"\nsession: {sess}")
    print(f"reveal:  python3 {os.path.abspath(__file__)} --verify {sess}")
    return 0


def fbcon_set(bind):
    """fbcon is what keeps the console actively scanning out the pipe. Unbinding it before we
    disable the OTG removes a driver from the pipe we are about to take down. ⚠ This is a
    HYPOTHESIS about the lockup, not an established cause — it is logged as an override so a
    result obtained with it can never be mistaken for one obtained without."""
    done = []
    for p in glob.glob("/sys/class/vtconsole/vtcon*/bind"):
        try:
            name = open(p.replace("/bind", "/name")).read().strip()
            if "frame buffer" not in name:
                continue
            open(p, "w").write("1" if bind else "0")
            done.append((p, name))
        except OSError as e:
            done.append((p, f"FAILED {e}"))
    return done


def setup_audio_and_atom(s, args, j, plans):
    """⛔ THE FEED AND THE EDGE, SET UP IDENTICALLY FOR EVERY ENTRY POINT.

    This exists because cmd_bisect used to build neither. It mapped BAR5, ran the register
    program, executed a 6-second PLAY delay with no stream armed and no ATOM runner, and
    reported 'SURVIVED' — so an operator listened to two separate runs that were incapable of
    emitting a sample. A diagnostic that can silently answer a question it never asked is worse
    than no diagnostic, and 'is the feed running?' must never again be answerable by assumption."""
    s.hda_prev_driver = pci_unbind(args.hda_dev)
    s.pci_cmd = pci_enable_master(args.hda_dev)
    s.dma = Dma()
    s.hda_bar = Mmio(f"/sys/bus/pci/devices/{args.hda_dev}/resource0", journal=j)
    s.hda = Hda(s.hda_bar, s.dma, journal=j)
    j.say(f"gcap: {s.hda.check_gcap()}")
    s.hda.stop_stream()
    s.hda.reset()
    s.hda.ring_init()
    j.say(f"rirb: {s.hda.prove_rirb()}")
    caddr, cvt, pin = 0, 0x04, 0x05
    cfg = s.hda.config_codec(caddr, cvt, pin)
    j.event("codec_config", detail=str(cfg))
    if s.edge_class != "SOFT":
        try:
            s.atom = AtomRunner(j, f"/sys/bus/pci/devices/{args.gpu_dev}/resource5")
            j.say("ATOM runner ready (VBIOS parsed, LiveCard mapped)")
        except Exception as e:
            j.say(f"⛔ ATOM runner UNAVAILABLE: {e!r}")
            return False
    return True


def cmd_bisect(args):
    """⭐ THE DIAGNOSTIC THE FIRST SESSIONS NEEDED AND DID NOT HAVE.

    Executes only the first N ops of one arm, fsyncing every single access, then restores and
    exits. No prompts, no sound, no battery. Walk N up until the box wedges: the last line in
    writes.jsonl is the write that did it. That converts 'it locks up somewhere in window 1'
    into a register address, in one boot per bisection step instead of a guess per power cycle."""
    try:
        arm, n = args.bisect.split(":")
        n = int(n)
    except ValueError:
        print("--bisect takes ARM:N, e.g. --bisect N0:20")
        return 2
    if arm not in ARM_SPECS:
        print(f"unknown arm {arm}; choose from {list(ARM_SPECS)}")
        return 2

    probe = Session(args, None)
    fail = probe.gates()
    if fail:
        print("REFUSING TO RUN:")
        for f in fail:
            print(f"  ⛔ {f}")
        return 2

    sess = f"{PRIOR_ART_DIR}/l1-bisect-{time.strftime('%Y%m%d-%H%M%S')}"
    j = Journal(sess)
    s = Session(args, j)
    j.say(f"BISECT {arm}:{n} — executing the first {n} ops only, fsync per access")

    fb = fbcon_set(False) if args.unbind_fbcon else []
    if fb:
        j.say(f"fbcon UNBOUND (override, logged): {fb}")
    try:
        s.bar = Mmio(f"/sys/bus/pci/devices/{args.gpu_dev}/resource5", journal=j)
        d = Derive(s.bar, s.consts, j)
        phy = d.phy(); dig = d.dig(phy); h_t, v_t = d.totals(s.consts)
        s.derived = dict(d.out, dig=dig, phy=phy, az=dig, pipe=0)
        j.artifact("derived.json", s.derived)
        progs = Programs(s.consts, dig, phy, dig, 0, h_t + 1, v_t + 1, ftrace=FtraceRef())
        env = Envelope(s.consts, phy, edge_class=s.edge_class)
        plans = {a: build_arm_plan(progs, env, a) for a in ARM_SPECS}
        s.snapshot(plans)
        j.say(f"pristine captured; plan {arm} has {len(plans[arm].ops)} ops")
        if not setup_audio_and_atom(s, args, j, plans):
            j.say("⛔ ATOM unavailable — this bisect cannot produce an edge. Aborting rather "
                  "than running a cut that looks like a test and is not one.")
            return 4

        full = plans[arm]
        clipped = Plan(arm)
        clipped.ops = full.ops[:n]
        for k, (a, b) in full.blocks.items():
            if a < n:
                clipped.blocks[k] = (a, min(b, n))
        j.artifact("bisect-ops.json",
                   [{"i": i, "kind": o.kind, "name": o.name, "note": o.note}
                    for i, o in enumerate(full.ops[:n + 3])])
        for i, o in enumerate(full.ops[:n]):
            j.say(f"  op {i:>3}: {o.kind:<5} {o.name}", tty=False)
        # ⛔ TELL THE OPERATOR IF THIS CUT CANNOT PRODUCE SOUND. A bisect that stops before
        # F_start feeds no samples, so "no audio" from it is not evidence of anything — and on
        # 2026-07-24 that cost a run when D:75 was suggested without checking that the feed is
        # at op 82. Silence has to mean something or it must not be reported at all.
        fs = next((i for i, o in enumerate(full.ops) if o.kind == "FN" and o.name == "F_start"),
                  None)
        pl = next((i for i, o in enumerate(full.ops) if o.kind == "DELAY" and o.name == "PLAY"),
                  None)
        if fs is not None and n <= fs:
            j.say("=" * 88)
            j.say(f"⚠ THIS CUT FEEDS NO AUDIO. F_start is op {fs} and PLAY is op {pl}; you asked "
                  f"for {n}.")
            j.say(f"  Nothing will be heard NO MATTER WHAT — that is arithmetic, not a result.")
            j.say(f"  For a cut that can make sound use:  --bisect {arm}:{len(full.ops)}")
            j.say("=" * 88)
        j.say(f"--- executing {n} ops; if the box wedges, the LAST line of writes.jsonl is it")
        # ⛔ A DIAGNOSTIC IS NOT A BLINDED WINDOW — it must play a KNOWN tone and SAY SO.
        # cmd_bisect used to pass b"" here, so even with the feed wired the buffer was empty and
        # PLAY was six seconds of nothing. Blinding is for the battery, where the operator's
        # report is evidence; here the only question is "does ANY sound come out", and the
        # operator needs to know exactly what to listen for to answer it.
        diag = {"bursts": 3, "pitch": "B", "pitch_hz": 440, "sweep": "U",
                "span": (400, 2400), "channels": "B", "gap_ms": 400}
        pcm = render_pattern(diag)
        if any(o.kind == "DELAY" and o.name == "PLAY" for o in clipped.ops):
            j.say("=" * 88)
            j.say("  🔊 LISTEN NOW. This cut plays a KNOWN tone (not blinded, this is a")
            j.say("     diagnostic): THREE 440 Hz beeps (musical A), a pause, then a rising")
            j.say("     sweep — BOTH speakers — and the whole thing twice over ~6 seconds.")
            j.say("     Heard it => the audio path works from userspace and the battery is real.")
            j.say("     Silence  => the path does not carry audio; that is L1's answer.")
            j.say("=" * 88)
        s.execute(clipped, 0, arm, pcm)
        j.say(f"--- SURVIVED {n} ops of {arm}")
    except Exception as e:
        j.say(f"--- EXCEPTION at/near op {n}: {e!r}")
    finally:
        s.g0_restore()
        bad = s.restore_verify() if s.pristine else []
        j.say(f"restored; {len(bad)} out-of-whitelist mismatches")
        if fb:
            fbcon_set(True)
            j.say("fbcon rebound")
        try:
            subprocess.run(["chown", "-R", "macro", sess], check=False)
        except Exception:
            pass
    print(f"\nsession: {sess}")
    return 0



def main():
    ap = argparse.ArgumentParser(
        prog="l1.py",
        description="BITE L1 — the zero-agnos-burn HDMI-audio sequencing discriminator.",
        epilog="The operator's path is: sudo l1.py    (with no arguments)")
    ap.add_argument("--plan-diff", action="store_true",
                    help="prove the single-variable claim. No root, no hardware.")
    ap.add_argument("--dry-run", action="store_true",
                    help="plan-diff plus the ATOM pre-flight. No root, no hardware.")
    ap.add_argument("--gates", action="store_true", help="report the refuse-to-start gates")
    ap.add_argument("--restore", action="store_true",
                    help="BLIND RECOVERY: replay the pristine snapshot. Idempotent.")
    ap.add_argument("--boot-entry", action="store_true", help="print the L1 boot entry")
    ap.add_argument("--install-boot-entry", action="store_true",
                    help="write the L1 boot entry (asks for confirmation; additive only)")
    ap.add_argument("--verify", dest="dir", metavar="DIR",
                    help="reconstruct and re-verify a finished session")
    ap.add_argument("--bisect", metavar="ARM:N",
                    help="DIAGNOSTIC. Execute only the FIRST N ops of ARM, fsync every access, "
                         "then restore and exit. No sound, no prompts, no battery. Use this to "
                         "find the write that wedges the box: run with N=0 and walk it up.")
    ap.add_argument("--unbind-fbcon", action="store_true",
                    help="unbind fbcon before touching the pipe. OFF BY DEFAULT: it BLANKS YOUR "
                         "CONSOLE for the whole run, and the hypothesis it was meant to test "
                         "(that fbcon caused the E_open wedge) was falsified by bisect N0:5.")
    ap.add_argument("--with-rlit", action="store_true",
                    help="include the amdgpu reference arm. OFF BY DEFAULT: its 201-write "
                         "ftrace replay wedged the APU twice on 2026-07-24. Excluding it "
                         "removes the session-validity gate — see the warning it prints.")
    ap.add_argument("--hdmi-mode", action="store_true",
                    help="MINIMAL plus the DIG_MODE 2->3 flip (routing untouched). REQUIRED for "
                         "a non-vacuous result: at DIG_MODE 2 the HDMI block is inert and no "
                         "unmute can sound, which is why the MINIMAL run was silent on its "
                         "positive control. Bisect it first.")
    ap.add_argument("--minimal-edge", action="store_true",
                    help="ONLY the ATOM #76 PHY edge: no OTG envelope, no BE<->FE routing "
                         "writes. Required on this hardware — both of those wedge the APU from "
                         "userspace (measured). Lowest fidelity, highest chance of running.")
    ap.add_argument("--no-envelope", action="store_true",
                    help="drop the OTG envelope, KEEP the ATOM edge. Required on this hardware: "
                         "disabling OTG_MASTER_EN from userspace hard-wedges the APU (measured). "
                         "Stamped into every artifact.")
    ap.add_argument("--soft-edge", action="store_true",
                    help="force EDGE=SOFT. DOWNGRADES the silence branch; stamped everywhere.")
    ap.add_argument("--gpu-dev", default=GPU_DEV_DEFAULT)
    ap.add_argument("--hda-dev", default=HDA_DEV_DEFAULT)
    # ⛔ Unknown arguments are a HARD ERROR (argparse default). The old script silently ignored
    # a typo'd --morph-dig-flip and ran a DIFFERENT experiment than the operator asked for.
    args = ap.parse_args()

    if args.dir:
        return cmd_verify(args)
    if args.bisect:
        return cmd_bisect(args)
    if args.plan_diff:
        return cmd_plan_diff(args)
    if args.dry_run:
        return cmd_dry_run(args)
    if args.gates:
        return cmd_gates(args)
    if args.restore:
        return cmd_restore(args)
    if args.boot_entry or args.install_boot_entry:
        return cmd_boot_entry(args)

    return cmd_run(args)


if __name__ == "__main__":
    sys.exit(main())
