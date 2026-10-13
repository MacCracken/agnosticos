#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# agnos-hdmi-linux.py — run AGNOS's ENTIRE HDMI-audio FEED process in Linux userspace,
# on the real archaemenid AMD hardware, WITHOUT the agnos kernel.
#
# WHY (operator's directive, 2026-07-16): "write the Cyrius Linux HDMI driver to confirm
# your process works without agnos." The register class is exhausted (every DIG/AZ/codec
# register byte-identical to amdgpu-playing), magnitude is exonerated (full-scale still
# silent), and the DCN enable SEQUENCE is exonerated (poke --replay-enable dropped audio
# only during the 120ms blank and it recovered). What is left is agnos's OWN FEED — the
# HDA controller DMA + codec bring-up on the 04:00.1 function — and the bare-metal
# environment. This driver reproduces agnos's feed EXACTLY (kernel/core/hda.cyr: CORB/RIRB
# verbs, SDn stream descriptor, 2-entry BDL, the 16-bit triangle tone) and sets the DCN
# side to the PROVEN-AUDIBLE known-good values (dcn-audio-known-good-full-0716.txt), so the
# ONLY variable under test is the feed. If sound comes out => agnos's feed logic is correct
# and the agnos-kernel ENVIRONMENT is the fault (DMA coherence / caching / MMIO ordering /
# IOMMU). If silent => agnos's feed logic itself is the bug, and we iterate HERE, by ear,
# with zero reflashes, until it makes sound — then port the delta into hda.cyr.
#
# ===========================  READ BEFORE RUNNING  ===========================
#  * Root required. It unbinds snd_hda_intel from 0000:04:00.1 and drives that HDA
#    controller RAW, and it writes GPU BAR5 (04:00.0) DCN audio registers behind amdgpu.
#  * It allocates a 2MB HUGEPAGE for DMA. Reserve one first:
#        echo 1 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages
#  * The DISPLAY should be unaffected (this touches only the audio function + AFMT audio
#    registers, not the pixel pipe). But amdgpu is not told any of this — REBOOT when done
#    to return to normal audio. Nothing is written to disk.
#  * Have the monitor's HDMI audio path live (its speakers/της line-out as usual).
#  * FREE THE CODEC FIRST or the unbind of snd_hda_intel blocks (D-state) while pipewire holds it:
#        systemctl --user stop wireplumber pipewire pipewire-pulse pipewire.socket pipewire-pulse.socket
#    The driver generates its OWN tone straight to the codec, so pipewire is not needed for playback.
#    Restart it after:  systemctl --user start pipewire pipewire-pulse wireplumber
# =============================================================================

import ctypes
import mmap
import os
import struct
import sys
import time


class _Tee:
    """Mirror everything printed (stdout AND stderr, incl. sys.exit messages) to a log file, line-flushed,
    so the operator never has to pipe — the run is always fully captured even when it exits early or loops."""
    def __init__(self, *streams):
        self.streams = streams

    def write(self, data):
        for s in self.streams:
            try:
                s.write(data); s.flush()
            except Exception:
                pass

    def flush(self):
        for s in self.streams:
            try:
                s.flush()
            except Exception:
                pass


def _start_logging(path):
    f = open(path, "w", buffering=1)
    sys.stdout = _Tee(sys.__stdout__, f)
    sys.stderr = _Tee(sys.__stderr__, f)
    return path

GPU_DEV = "0000:04:00.0"       # AMD DCN 2.1 (BAR5 = DCN registers)
HDA_DEV = "0000:04:00.1"       # AMD HDMI HDA controller (BAR0 = HDA MMIO)
DIG = 1                        # live encoder
CODEC = 0                      # ATI HDMI codec address on the 04:00.1 controller
CVT = 0x04                     # converter node (2nd digital pin's converter)
PIN = 0x05                     # pin node (ELD-valid, XB323U)
TAG = 3                        # stream tag (matches amdgpu AZ1 CHANNEL_STREAM_ID=0x30)
FMT = 0x11                     # 48kHz / 16-bit / 2ch  (matches amdgpu CONVERTER_FORMAT)

# ---- HDA controller register offsets (kernel/core/hda.cyr) ----
GCTL, GCTL_CRST = 0x08, 0x1
CORBLBASE, CORBUBASE, CORBWP, CORBRP = 0x40, 0x44, 0x48, 0x4A
CORBCTL, CORBSIZE = 0x4C, 0x4E
RIRBLBASE, RIRBUBASE, RIRBWP, RINTCNT = 0x50, 0x54, 0x58, 0x5A
RIRBCTL, RIRBSTS, RIRBSIZE = 0x5C, 0x5D, 0x5E
CORBRP_RST, RIRBWP_RST, CORBCTL_RUN, RIRBCTL_DMAEN, RING256 = 0x8000, 0x8000, 0x2, 0x2, 0x2
# output stream descriptor 0 (OSS starts after ISS; this controller ISS=0 => SD0 @ 0x80)
SD = 0x80
SD_CTL, SD_LPIB, SD_CBL, SD_LVI = 0x00, 0x04, 0x08, 0x0C
SD_FIFOS, SD_FMT, SD_BDPL, SD_BDPU = 0x10, 0x12, 0x18, 0x1C
SD_SRST, SD_RUN, SD_TAG_SHIFT = 0x1, 0x2, 20
BDL_IOC = 0x1

# ---- codec verbs (kernel/core/hda.cyr) ----
V_SET_STREAM_FMT = 0x2         # 4-bit verb, 16-bit payload
V_SET_STREAMID = 0x706         # 12-bit verb, 8-bit payload
V_SET_POWER = 0x705
V_SET_DIGI_CVT1 = 0x70D
V_SET_PINCTL = 0x707
V_SET_CVT_CHANS = 0x72D
V_ATI_DOWNMIX = 0x772
V_ATI_CHAN_ALLOC = 0x771
V_ATI_MCHAN_MODE = 0x789
V_ATI_MCHAN_01 = 0x777
V_ATI_MCHAN_1 = 0x785
DIGEN, PINCTL_OUT, ATI_MODE_SINGLE, ATI_OUT_EN = 0x01, 0x40, 0x1, 0x01

# ---- DCN BAR5 addressing (same as dump/poke tools) ----
SEG = [0x00000012, 0x000000C0, 0x000034C0, 0x00009000, 0x02403C00]
DIG_STRIDE = 0x100
def dcn2(off): return (SEG[2] + off + DIG * DIG_STRIDE) * 4     # DIG-relative, base_idx 2
def dcn1(off): return (SEG[1] + off) * 4                        # base_idx 1 (DCCG)

# The proven-audible known-good DIG1 DCN-audio state (dcn-audio-known-good-full-0716.txt),
# written verbatim so the DCN side is a CONSTANT and the feed is the only variable.
DCN_DIG = [  # (dword offset within DIG block, value)
    (0x20E6, 0x00000101),   # AFMT_CNTL (audio clock en/on)
    (0x2073, 0x00000010),   # HDMI_AUDIO_PACKET_CONTROL
    (0x2074, 0x00011000),   # HDMI_ACR_PACKET_CONTROL
    (0x2076, 0x00000010),   # HDMI_INFOFRAME_CONTROL0 (audio-info send)
    (0x2077, 0x00000200),   # HDMI_INFOFRAME_CONTROL1 (line)
    (0x2078, 0x00000003),   # HDMI_GENERIC_PACKET_CONTROL0 (AVI send+cont)
    (0x207B, 0x00000004),   # HDMI_GC
    (0x207C, 0x00000300),   # AFMT_AUDIO_PACKET_CONTROL2 (channel enable 0x3)
    (0x208C, 0x000D0282),   # AFMT_GENERIC_HDR (AVI header)
    (0x208D, 0x80885E8F),   # AFMT_GENERIC_0 (AVI body — amdgpu's YCbCr; video only)
    (0x208F, 0x000005CA),   # AFMT_GENERIC_2
    (0x2090, 0x00000AA1),   # AFMT_GENERIC_3
    (0x209B, 0x00001800),   # HDMI_ACR_48_1 (N=6144)
    (0x20A0, 0x00100000),   # AFMT_60958_0 (channel status L)
    (0x20A1, 0x00200000),   # AFMT_60958_1 (channel status R)
    (0x20A7, 0x00876543),   # AFMT_60958_2 (channels)
    (0x20B0, 0x00000101),   # DIG_BE_EN_CNTL (DIG_ENABLE + SYMCLK on)
    (0x20AC, 0x00000000),   # AFMT_INFOFRAME_CONTROL0
    (0x20AD, 0x00000001),   # AFMT_AUDIO_SRC_CONTROL (az endpoint 1)
]
DCN_AUDIO_PKT_CTL = 0x20AA     # AFMT_AUDIO_PACKET_CONTROL — sample-send opened LAST
DCN_AUDIO_PKT_VAL = 0x00000801
DCN_DTO = [(0x00AB, 0x00000000), (0x00AC, 0x0003A980), (0x00AD, 0x0024D998)]  # DTO src/phase/module


# ============================ low-level plumbing ============================
class Mmio:
    def __init__(self, path):
        self.fd = os.open(path, os.O_RDWR | os.O_SYNC)
        self.size = os.fstat(self.fd).st_size
        self.mm = mmap.mmap(self.fd, self.size, mmap.MAP_SHARED,
                            mmap.PROT_READ | mmap.PROT_WRITE)

    def r32(self, o): return struct.unpack_from("<I", self.mm, o)[0]
    def w32(self, o, v): struct.pack_into("<I", self.mm, o, v & 0xFFFFFFFF)
    def r16(self, o): return struct.unpack_from("<H", self.mm, o)[0]
    def w16(self, o, v): struct.pack_into("<H", self.mm, o, v & 0xFFFF)
    def r8(self, o): return self.mm[o]
    def w8(self, o, v): self.mm[o] = v & 0xFF


class Dma:
    """One 2MB physically-contiguous hugepage; sub-buffers at fixed offsets."""
    MAP_HUGETLB = 0x40000
    def __init__(self):
        self.size = 2 * 1024 * 1024
        self.buf = mmap.mmap(-1, self.size,
                             flags=mmap.MAP_PRIVATE | mmap.MAP_ANONYMOUS | self.MAP_HUGETLB,
                             prot=mmap.PROT_READ | mmap.PROT_WRITE)
        # touch + lock so the PFN is stable and present
        self.buf[0] = 0
        ctypes.CDLL(None, use_errno=True).mlock(
            ctypes.c_void_p(self._vaddr()), ctypes.c_size_t(self.size))
        self.phys = self._pagemap_phys(self._vaddr())
        if self.phys == 0:
            sys.exit("could not resolve hugepage physical address (need root + a reserved 2MB hugepage)")
        # SAFETY: the controller DMA-WRITES the RIRB to phys. Prove the whole 2MB is physically
        # contiguous (last 4KB page's PFN must be first + 511) before we ever arm RIRB DMA — a
        # discontiguity would scatter RIRB writes into unrelated RAM.
        last = self._pagemap_phys(self._vaddr() + self.size - 4096)
        if last != self.phys + self.size - 4096:
            sys.exit(f"hugepage not physically contiguous (phys 0x{self.phys:x}, last-page 0x{last:x}) — refusing to arm DMA")

    def _vaddr(self):
        return ctypes.addressof(ctypes.c_char.from_buffer(self.buf))

    @staticmethod
    def _pagemap_phys(vaddr):
        with open("/proc/self/pagemap", "rb") as f:
            f.seek((vaddr // 4096) * 8)
            entry = struct.unpack("<Q", f.read(8))[0]
        if not (entry & (1 << 63)):   # present bit
            return 0
        pfn = entry & ((1 << 55) - 1)
        return pfn * 4096 + (vaddr % 4096)

    def write32(self, off, v): struct.pack_into("<I", self.buf, off, v & 0xFFFFFFFF)
    def write16(self, off, v): struct.pack_into("<H", self.buf, off, v & 0xFFFF)


def pci_unbind(dev):
    drv = f"/sys/bus/pci/devices/{dev}/driver"
    if os.path.islink(drv):
        cur = os.path.basename(os.readlink(drv))
        print(f"  unbinding {dev} from {cur} ... (if this HANGS, the codec is held by pipewire — it will "
              f"self-release in ~1min via kernel timeouts, or stop pipewire first; see header)")
        with open(f"/sys/bus/pci/drivers/{cur}/unbind", "w") as f:
            f.write(dev)
        print(f"  unbound {dev} from {cur}")
        return cur
    print(f"  {dev} has no driver bound (already free)")
    return None


def pci_enable_master(dev):
    p = f"/sys/bus/pci/devices/{dev}/config"
    fd = os.open(p, os.O_RDWR)
    os.lseek(fd, 0x04, 0)
    cmd = struct.unpack("<H", os.read(fd, 2))[0]
    cmd |= 0x2 | 0x4                       # memory space + bus master
    os.lseek(fd, 0x04, 0)
    os.write(fd, struct.pack("<H", cmd))
    os.close(fd)


# ============================== HDA controller ==============================
class Hda:
    def __init__(self, bar, dma):
        self.b = bar
        self.dma = dma
        # DMA layout inside the hugepage
        self.corb_off, self.rirb_off, self.bdl_off, self.pcm_off = 0x0, 0x800, 0x1000, 0x10000
        self.corb_phys = dma.phys + self.corb_off
        self.rirb_phys = dma.phys + self.rirb_off
        self.bdl_phys = dma.phys + self.bdl_off
        self.pcm_phys = dma.phys + self.pcm_off
        self.corb_wp = 0
        self.rirb_rp = 0

    def imm_verb(self, nid, verb20):
        # DMA-FREE verb via the Immediate Command interface (ICOI 0x60 / ICII 0x64 / ICIS 0x68).
        # Bypasses CORB/RIRB entirely, so it works even if device DMA is blocked/remapped (IOMMU).
        # A valid response here + a dead RIRB round-trip = the fault is DMA addressing, not the codec.
        for _ in range(1000):
            if (self.b.r16(0x68) & 0x1) == 0: break            # wait ICB (busy) clear
            time.sleep(0.0001)
        cmd = (CODEC << 28) | (nid << 20) | (verb20 & 0xFFFFF)
        self.b.w32(0x60, cmd)                                   # ICOI
        self.b.w16(0x68, 0x1)                                   # ICIS: set ICB -> issue (clears IRV)
        for _ in range(1000):
            if self.b.r16(0x68) & 0x2:                          # IRV (result valid)
                return self.b.r32(0x64)                         # ICII
            time.sleep(0.0001)
        return None                                             # timed out — codec did not answer

    def probe_codec_immediate(self):
        resp = self.imm_verb(0x00, (0xF00 << 8) | 0x00)        # GET_PARAMETER VENDOR_ID, DMA-free
        if resp is None:
            print("  immediate-command probe: NO RESPONSE (ICIS IRV never set) — codec unreachable "
                  "(clock/power/address), NOT a DMA problem.")
            return None
        vendor = (resp >> 16) & 0xFFFF
        print(f"  immediate-command VENDOR_ID (DMA-free): resp=0x{resp:08x} vendor=0x{vendor:04x} "
              f"{'[ATI OK]' if vendor == 0x1002 else '[unexpected]'}")
        return vendor

    def check_gcap(self):
        # GCAP: [15:12]=OSS [11:8]=ISS [0]=64OK. Validate our hardcoded SD=0x80 and the 64-bit DMA gate.
        gcap = self.b.r16(0x00)
        oss, iss, ok64 = (gcap >> 12) & 0xF, (gcap >> 8) & 0xF, gcap & 0x1
        print(f"  GCAP=0x{gcap:04x}  OSS={oss} ISS={iss} 64OK={ok64}")
        if iss != 0 or oss < 1:
            sys.exit(f"unexpected stream layout (OSS={oss} ISS={iss}); SD offset 0x80 assumes ISS=0")
        top = self.dma.phys + self.dma.size
        if ok64 == 0 and top > 0x1_0000_0000:
            sys.exit(f"controller is 32-bit-only (GCAP 64OK=0) but hugepage phys 0x{self.dma.phys:x} is >4GB — "
                     f"RIRB writes would corrupt low RAM. Retry until a <4GB hugepage is handed out.")

    def reset(self):
        self.b.w32(GCTL, 0)                                   # into reset
        for _ in range(1000):
            if (self.b.r32(GCTL) & GCTL_CRST) == 0: break
            time.sleep(0.0001)
        time.sleep(0.0003)                                    # hold link in reset >100us (agnos holds 200us)
        self.b.w32(GCTL, GCTL_CRST)                           # out of reset
        for _ in range(1000):
            if (self.b.r32(GCTL) & GCTL_CRST) == GCTL_CRST: break
            time.sleep(0.0001)
        time.sleep(0.001)
        if (self.b.r32(GCTL) & GCTL_CRST) == 0:
            sys.exit("HDA controller did not come out of reset")

    def ring_init(self):
        self.b.w8(CORBCTL, 0)
        self.b.w8(RIRBCTL, 0)
        self.b.w32(CORBLBASE, self.corb_phys & 0xFFFFFFFF)
        self.b.w32(CORBUBASE, (self.corb_phys >> 32) & 0xFFFFFFFF)
        self.b.w8(CORBSIZE, RING256)
        self.b.w16(CORBRP, CORBRP_RST)                        # reset read ptr
        for _ in range(100):
            if self.b.r16(CORBRP) & CORBRP_RST: break
            time.sleep(0.0001)
        self.b.w16(CORBRP, 0)
        for _ in range(100):
            if (self.b.r16(CORBRP) & CORBRP_RST) == 0: break
            time.sleep(0.0001)
        self.b.w16(CORBWP, 0)
        self.b.w32(RIRBLBASE, self.rirb_phys & 0xFFFFFFFF)
        self.b.w32(RIRBUBASE, (self.rirb_phys >> 32) & 0xFFFFFFFF)
        self.b.w8(RIRBSIZE, RING256)
        self.b.w16(RIRBWP, RIRBWP_RST)                        # reset write ptr
        self.b.w16(RINTCNT, 0xFF)                             # match agnos (RINTCNT does not gate CORB drain)
        self.b.w8(RIRBCTL, RIRBCTL_DMAEN)
        self.b.w8(CORBCTL, CORBCTL_RUN)
        self.corb_wp, self.rirb_rp = 0, 0

    def prove_rirb(self):
        # KNOWN-ANSWER round-trip BEFORE any other verb: GET_PARAMETER(VENDOR_ID) must return the ATI
        # vendor in the high 16 bits. This proves (a) the controller's RIRB DMA lands in OUR buffer
        # (else we'd read a stale 0), and (b) a codec actually answered — the single 8-byte write that
        # catches a wrong-but-nonzero rirb_phys before it can corrupt RAM at stream rate.
        resp = self.verb(0x00, (0xF00 << 8) | 0x00)          # GET_PARAMETER, param 0x00 = VENDOR_ID
        rirbwp = self.b.r16(RIRBWP) & 0xFF
        if resp is None:
            print(f"  RIRB (DMA) VENDOR_ID round-trip: TIMEOUT (RIRBWP never advanced, RIRBWP={rirbwp})")
            vendor, resp = 0, 0
        else:
            vendor = (resp >> 16) & 0xFFFF
            print(f"  RIRB (DMA) VENDOR_ID round-trip: resp=0x{resp:08x} vendor=0x{vendor:04x}  (RIRBWP={rirbwp})")
        if vendor == 0x1002:
            return
        # RIRB failed. Cross-check with the DMA-free immediate-command path to localise the fault.
        imm = self.probe_codec_immediate()
        print("  --- diagnosis ---")
        if imm == 0x1002:
            sys.exit("  RIRB(DMA) DEAD but immediate-command WORKS => the codec is fine; the controller's DMA\n"
                     "  is not reaching our physical buffer. On this Linux box the IOMMU is almost certainly\n"
                     "  translating device DMA (agnos runs with amdvi DISABLED = raw physical). FIX: reboot with\n"
                     "  `amd_iommu=off` (or `iommu=pt`) on the kernel cmdline, then re-run — raw-physical DMA\n"
                     "  will then land and the real feed test can proceed. (Refusing to run the stream: a\n"
                     "  mis-landing DMA is the RAM-safety hazard this gate exists for.)")
        else:
            sys.exit("  RIRB(DMA) AND immediate-command BOTH dead => the codec itself is unreachable\n"
                     "  (unclocked/power-gated/wrong address), independent of DMA. amdgpu may have parked the\n"
                     "  HDMI-audio function when snd_hda_intel was absent. Refusing to run the stream.")

    def verb(self, nid, verb20):
        cmd = (CODEC << 28) | (nid << 20) | (verb20 & 0xFFFFF)
        self.corb_wp = (self.corb_wp + 1) & 0xFF
        self.dma.write32(self.corb_off + self.corb_wp * 4, cmd)
        self.b.w16(CORBWP, self.corb_wp)                      # doorbell
        deadline = time.monotonic() + 0.3                    # hard wall-clock cap so a mute verb can't hang the run
        while time.monotonic() < deadline:
            wp = self.b.r16(RIRBWP) & 0xFF
            if wp != self.rirb_rp:
                self.rirb_rp = (self.rirb_rp + 1) & 0xFF
                resp = struct.unpack_from("<I", self.dma.buf, self.rirb_off + self.rirb_rp * 8)[0]
                self.b.w8(RIRBSTS, 0x5)                       # W1C RINTFL+overrun
                return resp
        return None                                          # no response within the window

    def stop_stream(self):
        try:
            self.b.w32(SD + SD_CTL, 0)                        # clear RUN — quiesce DMA so the device isn't left wedged
        except Exception:
            pass

    def config_codec(self):
        # AGNOS's HDMI codec sequence (hda_codec_route digital branch + hda_hdmi_bind_single + ATI verbs)
        self.verb(0x01, (V_SET_POWER << 8) | 0x00)                       # AFG D0
        self.verb(CVT, (V_SET_POWER << 8) | 0x00)                        # converter D0 (hda.cyr:1059 — AFG-D0 does NOT cascade; a D3 converter = LPIB advances but SILENT)
        self.verb(CVT, (V_SET_DIGI_CVT1 << 8) | DIGEN)                   # digital enable
        self.verb(CVT, (V_SET_STREAM_FMT << 16) | FMT)                   # converter format
        self.verb(CVT, (V_SET_STREAMID << 8) | (TAG << 4))              # stream/channel id
        self.verb(CVT, (V_SET_CVT_CHANS << 8) | 0x01)                   # channels-1 = stereo (inert on ATI)
        self.verb(PIN, (V_SET_POWER << 8) | 0x00)                       # pin D0
        self.verb(PIN, (V_SET_PINCTL << 8) | PINCTL_OUT)               # pin out-enable
        self.verb(PIN, (V_ATI_DOWNMIX << 8) | 0x00)
        self.verb(PIN, (V_ATI_CHAN_ALLOC << 8) | 0x00)                  # CA=0 (FL/FR)
        self.verb(PIN, (V_ATI_MCHAN_MODE << 8) | ATI_MODE_SINGLE)      # SINGLE first
        self.verb(PIN, (V_ATI_MCHAN_01 << 8) | ATI_OUT_EN)            # slot0<-ch0
        self.verb(PIN, (V_ATI_MCHAN_1 << 8) | ((1 << 4) | ATI_OUT_EN))  # slot1<-ch1

    def fill_tone(self):
        # AGNOS's hda_refill_half: 16-bit triangle, amp 30000 (near full scale), phase-continuous.
        amp, acc, inc = 30000, 0, 300
        nframes = 0x10000 // 4                                          # 64KB ring / 4 bytes-per-frame
        for fi in range(nframes):
            a = acc & 0xFFFF
            s = (-amp + (2 * amp * a) // 32768) if a < 32768 else (amp - (2 * amp * (a - 32768)) // 32768)
            s &= 0xFFFF
            self.dma.write16(self.pcm_off + fi * 4, s)
            self.dma.write16(self.pcm_off + fi * 4 + 2, s)
            acc = (acc + inc) & 0xFFFF
            if (fi & 0x3F) == 0:
                inc += 1
                if inc > 1400: inc = 300

    def arm_stream(self):
        half = 0x8000
        # BDL: 2 x 32KB halves of the 64KB ring
        self.dma.write32(self.bdl_off + 0, self.pcm_phys & 0xFFFFFFFF)
        self.dma.write32(self.bdl_off + 4, (self.pcm_phys >> 32) & 0xFFFFFFFF)
        self.dma.write32(self.bdl_off + 8, half)
        self.dma.write32(self.bdl_off + 12, BDL_IOC)
        self.dma.write32(self.bdl_off + 16, (self.pcm_phys + half) & 0xFFFFFFFF)
        self.dma.write32(self.bdl_off + 20, ((self.pcm_phys + half) >> 32) & 0xFFFFFFFF)
        self.dma.write32(self.bdl_off + 24, half)
        self.dma.write32(self.bdl_off + 28, BDL_IOC)
        # stream reset
        self.b.w32(SD + SD_CTL, SD_SRST)
        for _ in range(1000):
            if self.b.r32(SD + SD_CTL) & SD_SRST: break
            time.sleep(0.00001)
        self.b.w32(SD + SD_CTL, 0)
        for _ in range(1000):
            if (self.b.r32(SD + SD_CTL) & SD_SRST) == 0: break
            time.sleep(0.00001)
        # program tag/CBL/FMT/LVI/BDL (RUN off)
        self.b.w32(SD + SD_CTL, TAG << SD_TAG_SHIFT)
        self.b.w32(SD + SD_CBL, half * 2)
        self.b.w16(SD + SD_FMT, FMT)
        self.b.w16(SD + SD_LVI, 1)
        self.b.w32(SD + SD_BDPL, self.bdl_phys & 0xFFFFFFFF)
        self.b.w32(SD + SD_BDPU, (self.bdl_phys >> 32) & 0xFFFFFFFF)
        # codec side must match SDnFMT
        self.verb(CVT, (V_SET_STREAM_FMT << 16) | FMT)
        self.verb(CVT, (V_SET_STREAMID << 8) | (TAG << 4))
        # RUN
        self.b.w32(SD + SD_CTL, (TAG << SD_TAG_SHIFT) | SD_RUN)
        l0 = self.b.r32(SD + SD_LPIB)
        time.sleep(0.05)
        l1 = self.b.r32(SD + SD_LPIB)
        print(f"  stream: tag={TAG} fmt=0x{FMT:x} lpib {l0}->{l1}  ({'ADVANCING' if l1 != l0 else 'STALLED'})")
        return l1 != l0


# =============================== DCN AFMT ===============================
def dcn_enable(bar5):
    for off, val in DCN_DTO:
        bar5.w32(dcn1(off), val)
    for off, val in DCN_DIG:
        bar5.w32(dcn2(off), val)
    # open the sample tap LAST (agnos ordering)
    bar5.w32(dcn2(DCN_AUDIO_PKT_CTL), DCN_AUDIO_PKT_VAL)
    print("  DCN AFMT set to known-good (proven-audible) state, sample tap open")


def main():
    if os.geteuid() != 0:
        sys.exit("root required")
    logpath = sys.argv[sys.argv.index("--log") + 1] if "--log" in sys.argv else "agnos-hdmi-linux.log"
    _start_logging(logpath)
    print(f"  [logging everything to {os.path.abspath(logpath)} — no piping needed]")
    print("=" * 74)
    print(" agnos-hdmi-linux — AGNOS's HDMI-audio FEED process, in Linux userspace, no agnos")
    print(" Reproduces hda.cyr (controller+codec+DMA feed) verbatim; DCN set to known-good.")
    print(" Sound => agnos's feed logic is correct, the bug is agnos's bare-metal environment.")
    print(" Silence => agnos's feed logic is the bug; iterate HERE by ear, no reflash.")
    print("=" * 74)

    prev = pci_unbind(HDA_DEV)
    print(f"  (rebind later with: echo {HDA_DEV} | sudo tee /sys/bus/pci/drivers/{prev}/bind  — or just reboot)")
    pci_enable_master(HDA_DEV)

    dma = Dma()
    print(f"  DMA hugepage phys=0x{dma.phys:x}")
    try:
        hda_bar = Mmio(f"/sys/bus/pci/devices/{HDA_DEV}/resource0")
    except OSError as e:
        sys.exit(f"  could not map the HDA controller BAR ({e}). The device is likely WEDGED from a prior run\n"
                 f"  that was killed mid-stream (DMA left active). REBOOT to clear it, then re-run — the new\n"
                 f"  always-stop-on-exit handling below prevents this recurring.")
    bar5 = Mmio(f"/sys/bus/pci/devices/{GPU_DEV}/resource5")

    hda = Hda(hda_bar, dma)
    hda.stop_stream()                                          # quiesce any stream left running by a prior killed run
    ok = False
    try:
        print("  [0] GCAP / 64-bit-DMA + stream-layout guard"); hda.check_gcap()
        print("  [1] controller reset");        hda.reset()
        print("  [2] CORB/RIRB ring init");      hda.ring_init()
        print("  [2b] RIRB DMA + codec round-trip (RAM-safety gate)"); hda.prove_rirb()
        print("  [3] codec config (HDMI cvt 0x04 / pin 0x05 + ATI slot map)"); hda.config_codec()
        print("  [4] fill ring with tone");      hda.fill_tone()
        print("  [5] DCN AFMT enable");          dcn_enable(bar5)
        print("  [6] arm + run stream");         ok = hda.arm_stream()
        print("=" * 74)
        morph = "--morph-dig-flip" in sys.argv
        if morph:
            print("  MORPH: will reproduce agnos's leaf-only DIG_MODE flip mid-playback (HDMI->DVI->HDMI,")
            print("         NO pipe/OTG re-commit). Listen: does the tone DIE at the flip and stay dead?")
        print(f"  stream {'RUNNING' if ok else 'STALLED'} — LISTEN NOW (playing ~20s, or Ctrl-C to stop early).")
        print("=" * 74)
        for i in range(10):                                    # ~20s then self-terminate (log finalizes)
            time.sleep(2)
            if morph and i == 2:                               # ~6s in, with audio confirmed, do agnos's flip
                DIG_BE_CNTL, DIG_MODE_MASK = 0x20AF, 0x70000    # bits[18:16]; DVI=2, HDMI=3
                be = bar5.r32(dcn2(DIG_BE_CNTL))
                print(f"    >>> MORPH FLIP NOW: DIG_BE_CNTL=0x{be:08x} (mode={(be>>16)&7}) -> DVI(2) -> HDMI(3), no re-commit")
                bar5.w32(dcn2(DIG_BE_CNTL), (be & ~DIG_MODE_MASK) | (2 << 16))   # -> DVI
                time.sleep(0.2)
                bar5.w32(dcn2(DIG_BE_CNTL), (be & ~DIG_MODE_MASK) | (3 << 16))   # -> HDMI (leaf only)
                print(f"    >>> flip done — DID THE TONE DIE? (agnos's exact move: leaf flip, pipe never re-committed)")
            print(f"    lpib={hda_bar.r32(SD + SD_LPIB)}  afmt_status=0x{bar5.r32(dcn2(0x20A9)):08x}")
    except KeyboardInterrupt:
        print("\n  stopped early (Ctrl-C)")
    finally:
        hda.stop_stream()                                      # ALWAYS quiesce DMA so the device is never left wedged
        print("  stream stopped, DMA quiesced — full log written. Reboot to restore normal audio.")


if __name__ == "__main__":
    main()
