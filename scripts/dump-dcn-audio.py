#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# dump-dcn-audio.py - dump AMD DCN 2.1 (Renoir/Cezanne, "green_sardine") display-audio
# registers from a live Linux system via BAR5 MMIO, read-only.
#
# Purpose: capture the known-good register state amdgpu leaves while HDMI audio plays,
# so AGNOS's sovereign DCN audio path can be diffed against reality.
#
# EVERY offset below is transcribed from, and cross-checked against, two independent sources:
#   [1] linux v6.6 drivers/gpu/drm/amd/include/asic_reg/dcn/dcn_2_1_0_offset.h
#   [2] linux v6.6 drivers/gpu/drm/amd/include/renoir_ip_offset.h   (DMU_BASE segments)
#   [3] umr database/ip/dcn_2_1_0.reg  (independent confirmation of offset + BASE_IDX)
#
# Address arithmetic (soc15 style):
#   BASE_IDX selects a segment of DMU_BASE.INST0:
#     DMU_BASE__INST0_SEG0 = 0x00000012
#     DMU_BASE__INST0_SEG1 = 0x000000C0   <- BASE_IDX 1 (DCCG / OTG pixel-rate)
#     DMU_BASE__INST0_SEG2 = 0x000034C0   <- BASE_IDX 2 (DIG encoders, Azalia endpoints)
#     DMU_BASE__INST0_SEG3 = 0x00009000
#     DMU_BASE__INST0_SEG4 = 0x02403C00
#   dword_addr = SEG[BASE_IDX] + offset      (offsets in the headers are DWORD indices)
#   byte_addr  = dword_addr * 4              (this is the offset into BAR5)
#
# Usage:
#   sudo ./dump-dcn-audio.py                 # safe: pure read-only, never writes
#   sudo ./dump-dcn-audio.py --az            # ALSO dump Azalia indexed regs (WRITES INDEX! see below)
#   sudo ./dump-dcn-audio.py --raw           # suppress bitfield decode (pure name/addr/value)
#   sudo ./dump-dcn-audio.py --dev 0000:04:00.0
#
# Safety:
#   Default mode opens BAR5 O_RDONLY and mmaps PROT_READ. A write is not merely
#   "not attempted" - it is impossible; the page is not writable. Safe with Hyprland
#   live and audio playing.
#
#   --az is DIFFERENT. See the warning printed by the script.

import argparse
import mmap
import os
import struct
import sys
import time

# ---------------------------------------------------------------------------
# DMU_BASE.INST0 segments -- renoir_ip_offset.h
#   static const struct IP_BASE DMU_BASE = { { { { 0x00000012, 0x000000C0,
#                                                 0x000034C0, 0x00009000,
#                                                 0x02403C00 } }, ...
# ---------------------------------------------------------------------------
SEG = [0x00000012, 0x000000C0, 0x000034C0, 0x00009000, 0x02403C00]


def addr(base_idx, off):
    """(BASE_IDX, dword offset) -> byte offset into BAR5."""
    return (SEG[base_idx] + off) * 4


# ---------------------------------------------------------------------------
# Bitfield decode tables, generated directly from dcn_2_1_0_sh_mask.h.
# Format: (shift, width, name)
# ---------------------------------------------------------------------------
FIELDS = {
    "DIG_FE_CNTL": [(0, 3, "DIG_SOURCE_SELECT"), (4, 3, "DIG_STEREOSYNC_SELECT"), (8, 1, "DIG_STEREOSYNC_GATE_EN"), (10, 1, "DIG_START"), (12, 3, "DIG_DIGITAL_BYPASS_SELECT"), (16, 2, "DIG_INPUT_PIXEL_SELECT"), (18, 1, "DOLBY_VISION_EN"), (24, 1, "DIG_SYMCLK_FE_ON"), (28, 1, "TMDS_PIXEL_ENCODING"), (30, 2, "TMDS_COLOR_FORMAT")],
    "DIG_BE_CNTL": [(0, 1, "DIG_DUAL_LINK_ENABLE"), (1, 1, "DIG_SWAP"), (2, 1, "DIG_RB_SWITCH_EN"), (8, 7, "DIG_FE_SOURCE_SELECT"), (16, 3, "DIG_MODE"), (28, 3, "DIG_HPD_SELECT")],
    "DIG_BE_EN_CNTL": [(0, 1, "DIG_ENABLE"), (8, 1, "DIG_SYMCLK_BE_ON")],
    "HDMI_CONTROL": [(0, 1, "HDMI_KEEPOUT_MODE"), (1, 1, "HDMI_DATA_SCRAMBLE_EN"), (2, 1, "HDMI_CLOCK_CHANNEL_RATE"), (3, 1, "HDMI_NO_EXTRA_NULL_PACKET_FILLED"), (4, 1, "HDMI_PACKET_GEN_VERSION"), (8, 1, "HDMI_ERROR_ACK"), (9, 1, "HDMI_ERROR_MASK"), (16, 6, "HDMI_UNSCRAMBLED_CONTROL_LINE_NUM"), (24, 1, "HDMI_DEEP_COLOR_ENABLE"), (28, 2, "HDMI_DEEP_COLOR_DEPTH")],
    "HDMI_AUDIO_PACKET_CONTROL": [(4, 2, "HDMI_AUDIO_DELAY_EN"), (8, 1, "HDMI_AUDIO_SEND_MAX_PACKETS"), (16, 5, "HDMI_AUDIO_PACKETS_PER_LINE")],
    "HDMI_ACR_PACKET_CONTROL": [(0, 1, "HDMI_ACR_SEND"), (1, 1, "HDMI_ACR_CONT"), (4, 2, "HDMI_ACR_SELECT"), (8, 1, "HDMI_ACR_SOURCE"), (12, 1, "HDMI_ACR_AUTO_SEND"), (16, 3, "HDMI_ACR_N_MULTIPLE"), (31, 1, "HDMI_ACR_AUDIO_PRIORITY")],
    "HDMI_INFOFRAME_CONTROL0": [(4, 1, "HDMI_AUDIO_INFO_SEND"), (5, 1, "HDMI_AUDIO_INFO_CONT"), (8, 1, "HDMI_MPEG_INFO_SEND"), (9, 1, "HDMI_MPEG_INFO_CONT")],
    "AFMT_AUDIO_PACKET_CONTROL": [(0, 1, "AFMT_AUDIO_SAMPLE_SEND"), (11, 1, "AFMT_RESET_FIFO_WHEN_AUDIO_DIS"), (12, 1, "AFMT_AUDIO_TEST_EN"), (14, 1, "AFMT_AUDIO_TEST_MODE"), (23, 1, "AFMT_AUDIO_FIFO_OVERFLOW_ACK"), (24, 1, "AFMT_AUDIO_CHANNEL_SWAP"), (26, 1, "AFMT_60958_CS_UPDATE"), (30, 1, "AFMT_AZ_AUDIO_ENABLE_CHG_ACK"), (31, 1, "AFMT_BLANK_TEST_DATA_ON_ENC_ENB")],
    "AFMT_AUDIO_PACKET_CONTROL2": [(0, 1, "AFMT_AUDIO_LAYOUT_OVRD"), (1, 1, "AFMT_AUDIO_LAYOUT_SELECT"), (8, 8, "AFMT_AUDIO_CHANNEL_ENABLE"), (16, 8, "AFMT_DP_AUDIO_STREAM_ID"), (24, 1, "AFMT_HBR_ENABLE_OVRD"), (28, 1, "AFMT_60958_OSF_OVRD")],
    "AFMT_CNTL": [(0, 1, "AFMT_AUDIO_CLOCK_EN"), (8, 1, "AFMT_AUDIO_CLOCK_ON")],
    "AFMT_STATUS": [(4, 1, "AFMT_AUDIO_ENABLE"), (8, 1, "AFMT_AZ_HBR_ENABLE"), (24, 1, "AFMT_AUDIO_FIFO_OVERFLOW"), (30, 1, "AFMT_AZ_AUDIO_ENABLE_CHG")],
    # HDMI_STATUS -- read-only hardware verdict. THE registers this whole arc has lacked.
    "HDMI_STATUS": [(0, 1, "HDMI_ACTIVE_AVMUTE"), (16, 1, "HDMI_AUDIO_PACKET_ERROR"), (20, 1, "HDMI_VBI_PACKET_ERROR"), (27, 1, "HDMI_ERROR_INT")],
    "AFMT_AUDIO_CRC_CONTROL": [(0, 1, "AFMT_AUDIO_CRC_EN"), (4, 1, "AFMT_AUDIO_CRC_CONT"), (8, 1, "AFMT_AUDIO_CRC_SOURCE"), (12, 4, "AFMT_AUDIO_CRC_CH_SEL"), (16, 16, "AFMT_AUDIO_CRC_COUNT")],
    "AFMT_AUDIO_CRC_RESULT": [(0, 1, "AFMT_AUDIO_CRC_DONE"), (8, 24, "AFMT_AUDIO_CRC")],
    "AFMT_AUDIO_SRC_CONTROL": [(0, 3, "AFMT_AUDIO_SRC_SELECT")],
    "AFMT_INFOFRAME_CONTROL0": [(6, 1, "AFMT_AUDIO_INFO_SOURCE"), (7, 1, "AFMT_AUDIO_INFO_UPDATE"), (10, 1, "AFMT_MPEG_INFO_UPDATE")],
    "DCCG_AUDIO_DTO_SOURCE": [(0, 3, "DCCG_AUDIO_DTO0_SOURCE_SEL"), (4, 2, "DCCG_AUDIO_DTO_SEL"), (12, 2, "DCCG_AUDIO_DTO2_SOURCE_SEL"), (16, 1, "DCCG_AUDIO_DTO2_CLOCK_EN"), (20, 1, "DCCG_AUDIO_DTO2_USE_512FBR_DTO"), (24, 1, "DCCG_AUDIO_DTO0_USE_512FBR_DTO"), (28, 1, "DCCG_AUDIO_DTO1_USE_512FBR_DTO")],
    "PIXEL_RATE_CNTL": [(0, 2, "PIXEL_RATE_SOURCE"), (4, 1, "DP_DTO_ENABLE"), (5, 1, "DP_DTO_DS_DISABLE"), (8, 1, "ADD_PIXEL"), (9, 1, "DROP_PIXEL"), (11, 1, "DISPOUT_HALF_RATE_EN"), (14, 2, "DIO_FIFO_ERROR"), (16, 12, "DIO_ERROR_COUNT")],
    "HDMI_GENERIC_PACKET_CONTROL0": [(0, 1, "HDMI_GENERIC0_SEND"), (1, 1, "HDMI_GENERIC0_CONT"), (2, 1, "HDMI_GENERIC0_LINE_REFERENCE"), (4, 1, "HDMI_GENERIC1_SEND"), (5, 1, "HDMI_GENERIC1_CONT"), (8, 1, "HDMI_GENERIC2_SEND"), (9, 1, "HDMI_GENERIC2_CONT"), (12, 1, "HDMI_GENERIC3_SEND"), (13, 1, "HDMI_GENERIC3_CONT")],
    "AFMT_VBI_PACKET_CONTROL": [(0, 1, "AFMT_GENERIC_LOCK_STATUS"), (4, 1, "AFMT_GENERIC_CONFLICT"), (17, 1, "AFMT_GENERIC_CONFLICT_CLR"), (28, 4, "AFMT_GENERIC_INDEX")],
}

# DIG_MODE encoding, from dcn10_link_encoder.c:dcn10_link_encoder_setup()
DIG_MODE_NAMES = {0: "DP-SST", 1: "LVDS", 2: "TMDS-DVI", 3: "TMDS-HDMI", 5: "DP-MST"}

# ---------------------------------------------------------------------------
# DCCG audio DTO block -- BASE_IDX 1 (SEG1 = 0xC0)
#   #define mmDCCG_AUDIO_DTO_SOURCE   0x00ab  / _BASE_IDX 1
#   #define mmDCCG_AUDIO_DTO0_PHASE   0x00ac  / _BASE_IDX 1
#   #define mmDCCG_AUDIO_DTO0_MODULE  0x00ad  / _BASE_IDX 1
#   #define mmDCCG_AUDIO_DTO1_PHASE   0x00ae  / _BASE_IDX 1
#   #define mmDCCG_AUDIO_DTO1_MODULE  0x00af  / _BASE_IDX 1
# ---------------------------------------------------------------------------
DCCG_REGS = [
    ("DCCG_AUDIO_DTO_SOURCE",  1, 0x00AB, "DCCG_AUDIO_DTO_SOURCE"),
    ("DCCG_AUDIO_DTO0_PHASE",  1, 0x00AC, None),
    ("DCCG_AUDIO_DTO0_MODULE", 1, 0x00AD, None),
    ("DCCG_AUDIO_DTO1_PHASE",  1, 0x00AE, None),
    ("DCCG_AUDIO_DTO1_MODULE", 1, 0x00AF, None),
]

# ---------------------------------------------------------------------------
# OTG pixel-rate / DP DTO -- BASE_IDX 1. Stride 4 dwords per OTG instance.
#   #define mmOTG0_PIXEL_RATE_CNTL         0x0080 / _BASE_IDX 1
#   #define mmDP_DTO0_PHASE                0x0081 / _BASE_IDX 1
#   #define mmDP_DTO0_MODULO               0x0082 / _BASE_IDX 1   (MODULO, not MODULE)
#   #define mmOTG0_PHYPLL_PIXEL_RATE_CNTL  0x0083 / _BASE_IDX 1
# ---------------------------------------------------------------------------
OTG_BASE = 0x0080
OTG_STRIDE = 4


def otg_regs():
    out = []
    for i in range(4):
        b = OTG_BASE + i * OTG_STRIDE
        out += [
            (f"OTG{i}_PIXEL_RATE_CNTL",        1, b + 0, "PIXEL_RATE_CNTL"),
            (f"DP_DTO{i}_PHASE",               1, b + 1, None),
            (f"DP_DTO{i}_MODULO",              1, b + 2, None),
            (f"OTG{i}_PHYPLL_PIXEL_RATE_CNTL", 1, b + 3, "PIXEL_RATE_CNTL"),
        ]
    return out


# ---------------------------------------------------------------------------
# DIG encoder block -- BASE_IDX 2 (SEG2 = 0x34C0). DIG0 offsets below.
# DCN 2.1 has FIVE DIG instances (DIG0..DIG4); stride = 0x100 dwords, verified:
#   mmDIG0_DIG_BE_CNTL 0x20af / mmDIG1_DIG_BE_CNTL 0x21af / ... / mmDIG4_DIG_BE_CNTL 0x24af
# Each entry: (suffix, DIG0 dword offset, decode key)
# ---------------------------------------------------------------------------
DIG_STRIDE = 0x100
DIG_REGS = [
    ("DIG_FE_CNTL",                  0x2068, "DIG_FE_CNTL"),
    ("HDMI_CONTROL",                 0x2071, "HDMI_CONTROL"),
    # HDMI_STATUS -- READ-ONLY, and the only register on this block that is a hardware VERDICT rather than an
    # echo of a driver write. HDMI_ACTIVE_AVMUTE[0], HDMI_AUDIO_PACKET_ERROR[16], HDMI_VBI_PACKET_ERROR[20],
    # HDMI_ERROR_INT[27]. agnos has never read it and neither had this script. If agnos's HDMI_AUDIO_PACKET_ERROR
    # is set and amdgpu's is clear while playing, that is the first non-echo evidence the arc has had.
    ("HDMI_STATUS",                  0x2072, "HDMI_STATUS"),
    ("HDMI_AUDIO_PACKET_CONTROL",    0x2073, "HDMI_AUDIO_PACKET_CONTROL"),
    ("HDMI_ACR_PACKET_CONTROL",      0x2074, "HDMI_ACR_PACKET_CONTROL"),
    ("HDMI_VBI_PACKET_CONTROL",      0x2075, None),
    ("HDMI_INFOFRAME_CONTROL0",      0x2076, "HDMI_INFOFRAME_CONTROL0"),
    ("HDMI_INFOFRAME_CONTROL1",      0x2077, None),
    ("HDMI_GENERIC_PACKET_CONTROL0", 0x2078, "HDMI_GENERIC_PACKET_CONTROL0"),
    ("HDMI_GC",                      0x207B, None),
    ("AFMT_AUDIO_PACKET_CONTROL2",   0x207C, "AFMT_AUDIO_PACKET_CONTROL2"),
    # --- The AVI InfoFrame payload + generic-packet plumbing. THE GAP: agnos writes all of these
    # from a clean-room derivation with NO known-good to compare against, and the DCN egress is now
    # the only place the residual can live (DMA proven running, both halves proven configured).
    # On DCN 2.x the AVI IS generic packet slot 0 (dcn20_stream_encoder.c enc2_update_hdmi_info_packet).
    ("AFMT_GENERIC_HDR",             0x208C, None),   # HB0[7:0] HB1[15:8] HB2[23:16] HB3[31:24]
    ("AFMT_GENERIC_0",               0x208D, None),   # sb[0..3] — sb[0] is the AVI checksum
    ("AFMT_GENERIC_1",               0x208E, None),
    ("AFMT_GENERIC_2",               0x208F, None),
    ("AFMT_GENERIC_3",               0x2090, None),
    ("AFMT_GENERIC_4",               0x2091, None),
    ("AFMT_GENERIC_5",               0x2092, None),
    ("AFMT_GENERIC_6",               0x2093, None),
    ("AFMT_GENERIC_7",               0x2094, None),
    ("HDMI_GENERIC_PACKET_CONTROL1", 0x2095, None),   # GENERIC0_LINE lives HERE, not in CONTROL0
    ("AFMT_VBI_PACKET_CONTROL",      0x20AB, "AFMT_VBI_PACKET_CONTROL"),  # the GSP index — NOT 0x2075
    ("AFMT_VBI_PACKET_CONTROL1",     0x20E7, None),   # GENERIC0_IMMEDIATE_UPDATE
    ("HDMI_DB_CONTROL",              0x2088, None),   # double-buffer control for the HDMI packet block
    # ACR_STATUS -- read-only; whether the audio-clock-regeneration engine is locked and what CTS it measured.
    # agnos deliberately leaves the CTS registers unwritten (HDMI_ACR_SOURCE=0 => hardware-measured CTS), and
    # that reasoning survived audit -- but nobody has ever READ what the hardware actually measured.
    ("HDMI_ACR_STATUS_0",            0x209C, None),
    ("HDMI_ACR_STATUS_1",            0x209D, None),
    ("HDMI_ACR_32_0",                0x2096, None),
    ("HDMI_ACR_32_1",                0x2097, None),
    ("HDMI_ACR_44_0",                0x2098, None),
    ("HDMI_ACR_44_1",                0x2099, None),
    ("HDMI_ACR_48_0",                0x209A, None),
    ("HDMI_ACR_48_1",                0x209B, None),
    ("AFMT_AUDIO_INFO0",             0x209E, None),
    ("AFMT_AUDIO_INFO1",             0x209F, None),
    ("AFMT_60958_0",                 0x20A0, None),
    ("AFMT_60958_1",                 0x20A1, None),
    ("AFMT_60958_2",                 0x20A7, None),
    # The audio CRC pair — the only registers on this block that report what the hardware DID rather than
    # what it was told. CRC_DONE asserts only when CRC_COUNT samples have physically traversed the AFMT, and
    # the CRC value is content-derived. Captured here so agnos's probe has a known-good to mean anything
    # against: on a working amdgpu path we expect CONTROL == 0 (amdgpu's DCN path never arms it) and RESULT
    # == 0, which is itself the finding — it tells us the block is idle unless someone arms it, so agnos's
    # own reading is self-contained rather than a diff.
    ("AFMT_AUDIO_CRC_CONTROL",       0x20A2, "AFMT_AUDIO_CRC_CONTROL"),
    ("AFMT_AUDIO_CRC_RESULT",        0x20A8, "AFMT_AUDIO_CRC_RESULT"),
    # The last unread AFMT registers. Free riders: four extra MMIO reads that close the AFMT register file
    # EXHAUSTIVELY, so no future session repeats the set-subtraction (which has now been done twice and got
    # a different answer each time -- the authoritative count is 40 mmDIG0_AFMT_* defs in
    # dcn_2_1_0_offset.h, 24 known to agnos, 16 unknown, 5 genuinely residual).
    # RAMP_CONTROL* — RETRACTION (2026-07-16): the old note here said "none of these is a candidate cause
    # (test-tone generator gated by AFMT_AUDIO_TEST_EN)". That was inference from a field name, not a
    # measurement, and it is now a LIVE candidate: agnos reads ALL FOUR = 0 (never programmed; the GOP does
    # not touch them), while radeon's MMIO HDMI-audio lineage (r600_hdmi.c / evergreen_hdmi.c, the same AFMT
    # family) programs 0xFFFFFF/0x7FFFFF/1/1 on every audio enable. amdgpu-DCN writes them via no traced
    # MMIO — consistent with DMUB/vBIOS-init programming that the register-diff corpus never captured.
    # A ramp ceiling of 0 = output attenuated to exact silence with every upstream instrument reading
    # healthy. >>> The ONE decisive capture this script can still take: run it under amdgpu WHILE AUDIO IS
    # AUDIBLY PLAYING and read these four rows — non-zero confirms the mute mechanism, 0 kills it. <<<
    ("AFMT_RAMP_CONTROL0",           0x20A3, None),   # RAMP_MAX_COUNT[23:0], RAMP_DATA_SIGN[31]
    ("AFMT_RAMP_CONTROL1",           0x20A4, None),   # RAMP_MIN_COUNT[23:0], AUDIO_TEST_CH_DISABLE[31:24]
    ("AFMT_RAMP_CONTROL2",           0x20A5, None),   # RAMP_INC_COUNT[23:0]
    ("AFMT_RAMP_CONTROL3",           0x20A6, None),   # RAMP_DEC_COUNT[23:0]
    ("AFMT_INTERRUPT_STATUS",        0x2079, None),   # opaque: zero decoded bitfields in dcn_2_1_0_sh_mask.h
    ("AFMT_STATUS",                  0x20A9, "AFMT_STATUS"),
    ("AFMT_AUDIO_PACKET_CONTROL",    0x20AA, "AFMT_AUDIO_PACKET_CONTROL"),
    ("AFMT_INFOFRAME_CONTROL0",      0x20AC, "AFMT_INFOFRAME_CONTROL0"),
    ("AFMT_AUDIO_SRC_CONTROL",       0x20AD, "AFMT_AUDIO_SRC_CONTROL"),
    ("DIG_BE_CNTL",                  0x20AF, "DIG_BE_CNTL"),
    ("DIG_BE_EN_CNTL",               0x20B0, "DIG_BE_EN_CNTL"),
    ("AFMT_CNTL",                    0x20E6, "AFMT_CNTL"),
]

# ---------------------------------------------------------------------------
# Azalia endpoints -- BASE_IDX 2. EIGHT endpoints (AZF0ENDPOINT0..7).
#
# *** STRIDE IS 6 DWORDS (0x18 BYTES) -- NOT 4, NOT 8. ***
#   #define mmAZF0ENDPOINT0_AZALIA_F0_CODEC_ENDPOINT_INDEX 0x0386  ("base address: 0x0")
#   #define mmAZF0ENDPOINT1_AZALIA_F0_CODEC_ENDPOINT_INDEX 0x038c  ("base address: 0x18")
#   #define mmAZF0ENDPOINT2_AZALIA_F0_CODEC_ENDPOINT_INDEX 0x0392  ("base address: 0x30")
# The header's own addressBlock comments give byte bases 0x0/0x18/0x30 => 0x18 bytes
# => 6 dwords per endpoint. DATA is always INDEX+1.
# ---------------------------------------------------------------------------
AZ_INDEX0 = 0x0386
AZ_STRIDE = 6
AZ_N = 8

# Indexed ordinals within an endpoint window. Identical for every endpoint --
# the endpoint is chosen by WHICH INDEX/DATA pair you use, not by the ordinal.
#   #define ixAZF0ENDPOINT0_AZALIA_F0_CODEC_CONVERTER_CONTROL_CHANNEL_STREAM_ID  0x0003
#   #define ixAZF0ENDPOINT0_AZALIA_F0_CODEC_PIN_CONTROL_CHANNEL_SPEAKER          0x0025
#   #define ixAZF0ENDPOINT0_AZALIA_F0_CODEC_PIN_CONTROL_HOT_PLUG_CONTROL         0x0054
#   #define ixAZF0ENDPOINT0_AZALIA_F0_CODEC_PIN_CONTROL_RESPONSE_CONFIGURATION_DEFAULT 0x0056
AZ_ORDINALS = [
    ("CONVERTER_PARAMETER_AUDIO_WIDGET_CAPABILITIES", 0x0001),
    ("CONVERTER_CONTROL_CONVERTER_FORMAT",            0x0002),
    ("CONVERTER_CONTROL_CHANNEL_STREAM_ID",           0x0003),
    ("CONVERTER_CONTROL_DIGITAL_CONVERTER",           0x0004),
    ("CONVERTER_PARAMETER_STREAM_FORMATS",            0x0005),
    ("CONVERTER_PARAMETER_SUPPORTED_SIZE_RATES",      0x0006),
    ("PIN_PARAMETER_CAPABILITIES",                    0x0021),
    ("PIN_CONTROL_UNSOLICITED_RESPONSE",              0x0022),
    ("PIN_CONTROL_RESPONSE_PIN_SENSE",                0x0023),
    ("PIN_CONTROL_WIDGET_CONTROL",                    0x0024),
    ("PIN_CONTROL_CHANNEL_SPEAKER",                   0x0025),
    ("PIN_CONTROL_AUDIO_DESCRIPTOR0",                 0x0028),
    ("PIN_CONTROL_AUDIO_DESCRIPTOR1",                 0x0029),
    ("PIN_CONTROL_AUDIO_DESCRIPTOR2",                 0x002A),
    ("PIN_CONTROL_MULTICHANNEL_ENABLE",               0x0036),
    ("PIN_CONTROL_RESPONSE_LIPSYNC",                  0x0037),
    ("PIN_CONTROL_RESPONSE_HBR",                      0x0038),
    ("PIN_CONTROL_SINK_INFO0",                        0x003A),
    ("PIN_CONTROL_SINK_INFO1",                        0x003B),
    ("PIN_CONTROL_SINK_INFO2",                        0x003C),
    ("PIN_CONTROL_SINK_INFO3",                        0x003D),
    ("PIN_CONTROL_SINK_INFO4",                        0x003E),
    ("PIN_CONTROL_SINK_INFO5",                        0x003F),
    ("PIN_CONTROL_SINK_INFO6",                        0x0040),
    ("PIN_CONTROL_SINK_INFO7",                        0x0041),
    ("PIN_CONTROL_SINK_INFO8",                        0x0042),
    ("PIN_CONTROL_HOT_PLUG_CONTROL",                  0x0054),
    ("PIN_CONTROL_RESPONSE_CONFIGURATION_DEFAULT",    0x0056),
    ("PIN_CONTROL_MULTICHANNEL_MODE",                 0x0058),
    ("PIN_CONTROL_DIGITAL_OUTPUT_STATUS",             0x0063),
    ("PIN_CONTROL_LPIB",                              0x0065),
    ("PIN_CONTROL_CODING_TYPE",                       0x0067),
    ("PIN_CONTROL_FORMAT_CHANGED",                    0x0068),
    ("AUDIO_ENABLE_STATUS",                           0x006B),
]


class Bar:
    def __init__(self, path, writable=False):
        self.writable = writable
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
        if not self.writable:
            raise RuntimeError("BAR mapped read-only")
        struct.pack_into("<I", self.mm, byte_off, val)

    def close(self):
        self.mm.close()
        os.close(self.fd)


def decode(key, val):
    out = []
    for shift, width, name in FIELDS.get(key, []):
        v = (val >> shift) & ((1 << width) - 1)
        hi = shift + width - 1
        rng = f"[{hi}:{shift}]" if width > 1 else f"[{shift}]"
        extra = ""
        if name == "DIG_MODE":
            extra = f"  <- {DIG_MODE_NAMES.get(v, 'RESERVED/INVALID')}"
        out.append(f"      {name:<40} {rng:>8} = 0x{v:x}{extra}")
    return out


def emit(bar, name, base_idx, off, key, raw):
    a = addr(base_idx, off)
    v = bar.rd(a)
    if v is None:
        print(f"  {name:<46} 0x{a:06x}  <OUT-OF-BAR>")
        return
    print(f"  {name:<46} 0x{a:06x}  0x{v:08x}")
    if not raw:
        for line in decode(key, v):
            print(line)


def main():
    ap = argparse.ArgumentParser(description="Dump DCN 2.1 display-audio registers (read-only).")
    ap.add_argument("--dev", default="0000:04:00.0", help="PCI address of the GPU (default 0000:04:00.0)")
    ap.add_argument("--raw", action="store_true", help="suppress bitfield decode")
    ap.add_argument("--az", action="store_true",
                    help="ALSO dump Azalia indexed registers. THIS WRITES the AZ INDEX register. See warning.")
    ap.add_argument("--force-az", action="store_true", help="skip the --az confirmation prompt")
    ap.add_argument("--crc", type=int, metavar="DIG",
                    help="arm the AFMT audio CRC on DIG<n> over --crc-count samples and report the result. "
                         "WRITES AFMT_AUDIO_CRC_CONTROL (see the warning). Play audio to the sink first.")
    ap.add_argument("--crc-count", type=int, default=2048, help="samples to checksum (default 2048 = ~43ms @48kHz)")
    ap.add_argument("--crc-source", type=int, default=0, choices=[0, 1],
                    help="AFMT_AUDIO_CRC_SOURCE (default 0 -- the value agnos uses; run BOTH to learn what it selects)")
    ap.add_argument("--az-write", metavar="EP:ORD:VAL", action="append", default=[],
                    help="Write an Azalia endpoint indexed ordinal: --az-write 1:0x02:0x11 . Repeatable. "
                         "WRITES. Used to PERTURB the working amdgpu path toward agnos's config and find "
                         "which single value kills the sound.")
    ap.add_argument("--reg-write", metavar="BASE:OFF:VAL", action="append", default=[],
                    help="Write any BAR5 register: --reg-write 1:0x00AD:0x24D9B8 (BASE_IDX:dword_off:val). "
                         "Repeatable. WRITES.")
    ap.add_argument("--sample-send", metavar="DIG:VAL",
                    help="RMW AFMT_AUDIO_PACKET_CONTROL.AFMT_AUDIO_SAMPLE_SEND (bit0) on DIG<n> to VAL. "
                         "This is amdgpu's OWN mute lever (enc1_se_audio_mute_control is literally "
                         "REG_UPDATE(AFMT_AUDIO_PACKET_CONTROL, AFMT_AUDIO_SAMPLE_SEND, !mute)); "
                         "dcn20_stream_encoder.c:619 binds it, so it is Cezanne's live path. Fully "
                         "reversible. WRITES.")
    ap.add_argument("--fifo-reset-bit", metavar="DIG:VAL",
                    help="RMW AFMT_AUDIO_PACKET_CONTROL.AFMT_RESET_FIFO_WHEN_AUDIO_DIS (bit11) on DIG<n>. "
                         "The confound control for --sample-send: 0x801 has bit11 SET, so muting alone may "
                         "hold the FIFO in reset -- which is NOT agnos's state (agnos's FIFO OVERFLOWS, "
                         "i.e. is not reset). WRITES.")
    ap.add_argument("--pipe", type=int, default=None, metavar="N",
                    help="ALSO range-dump the OTG<N> + DCCG pipe/clock blocks (READ-ONLY). Use this to capture "
                         "what amdgpu's cold HDMI modeset set in the pipe/clock registers agnos never touches "
                         "(agnos inherits the GOP's DVI values). Diff the result against agnos's gpu.cyr writes "
                         "to find the missing modeset step. N = the display OTG (0 on archaemenid).")
    args = ap.parse_args()

    if os.geteuid() != 0:
        sys.exit("error: must run as root (BAR5 mmap requires CAP_SYS_ADMIN)")

    res = f"/sys/bus/pci/devices/{args.dev}/resource5"
    if not os.path.exists(res):
        sys.exit(f"error: {res} not found - wrong --dev?")

    if args.az and not args.force_az:
        sys.stderr.write(
            "\n"
            "!!  --az writes AZALIA_F0_CODEC_ENDPOINT_INDEX to select each ordinal.\n"
            "!!  amdgpu uses the SAME index/data window. If amdgpu touches an endpoint\n"
            "!!  (hotplug, mode set, stream enable) between our INDEX write and DATA read,\n"
            "!!  or we clobber an INDEX it is mid-transaction on, audio state can corrupt.\n"
            "!!  We never write DATA, so nothing is reprogrammed - the risk is a race on INDEX.\n"
            "!!  Do not touch displays/audio devices while this runs. Type 'yes' to proceed: "
        )
        if input().strip().lower() != "yes":
            sys.exit("aborted")

    bar = Bar(res, writable=(args.az or args.crc is not None
                             or args.sample_send is not None or args.fifo_reset_bit is not None
                             or args.az_write or args.reg_write))

    print("# DCN 2.1 display-audio register dump (Renoir/Cezanne, umr asic name: green_sardine)")
    print(f"# dev={args.dev} bar5_size=0x{bar.size:x} time={time.strftime('%Y-%m-%dT%H:%M:%S')}")
    print(f"# SEG1(BASE_IDX 1)=0x{SEG[1]:x}  SEG2(BASE_IDX 2)=0x{SEG[2]:x}  byte=(SEG+off)*4")
    print(f"# az_indexed={'YES (INDEX written)' if args.az else 'no (pure read-only)'}")
    print()

    print("== DCCG audio DTO (BASE_IDX 1) ==")
    for n, b, o, k in DCCG_REGS:
        emit(bar, n, b, o, k, args.raw)
    print()

    print("== OTG pixel rate / DP DTO (BASE_IDX 1) ==")
    for n, b, o, k in otg_regs():
        emit(bar, n, b, o, k, args.raw)
    print()

    for inst in range(5):
        print(f"== DIG{inst} encoder (BASE_IDX 2) ==")
        for suffix, off0, k in DIG_REGS:
            emit(bar, f"DIG{inst}_{suffix}", 2, off0 + inst * DIG_STRIDE, k, args.raw)
        print()

    print("== Azalia endpoint INDEX/DATA window regs (BASE_IDX 2, stride 6 dwords) ==")
    for i in range(AZ_N):
        io = AZ_INDEX0 + i * AZ_STRIDE
        emit(bar, f"AZF0ENDPOINT{i}_ENDPOINT_INDEX", 2, io, None, args.raw)
        emit(bar, f"AZF0ENDPOINT{i}_ENDPOINT_DATA", 2, io + 1, None, args.raw)
    print()

    if args.pipe is not None:
        # DISABLED — DO NOT RE-ENABLE AS A RANGE SCAN. A blind read-sweep of the OTG (BASE_IDX 2 0x1B00+) and
        # DCCG (BASE_IDX 1 0x00..0xC0) blocks LOCKED THE WHOLE MACHINE on archaemenid (2026-07-17): some
        # registers in those ranges sit behind gated clock domains and reading them hangs the bus (same hazard
        # class as the DMU/DMCUB mailbox at seg3 0x9000 — see memory). GPU register READS are NOT universally
        # safe. Any future pipe capture must read ONLY specific offsets verified against dcn_2_1_0_offset.h and
        # known to be in an ungated domain — never a range sweep.
        sys.exit("--pipe is disabled: a range read-sweep of the OTG/DCCG blocks hangs the machine (gated-clock "
                 "registers). Do not blind-scan GPU registers. Use verified single offsets only.")

    if args.az:
        print("== Azalia indexed ordinals (INDEX written, DATA read) ==")
        for i in range(AZ_N):
            idx_a = addr(2, AZ_INDEX0 + i * AZ_STRIDE)
            dat_a = addr(2, AZ_INDEX0 + i * AZ_STRIDE + 1)
            saved = bar.rd(idx_a)
            print(f"  -- endpoint {i} (INDEX@0x{idx_a:06x} DATA@0x{dat_a:06x}, saved INDEX=0x{saved:08x})")
            for oname, ordinal in AZ_ORDINALS:
                bar.wr(idx_a, ordinal)
                v = bar.rd(dat_a)
                print(f"  AZ{i}.{oname:<48} ix0x{ordinal:04x}  0x{v:08x}")
            # restore whatever amdgpu had selected, to shrink the race window
            bar.wr(idx_a, saved)
            print()

    # --- PERTURBATION: break the WORKING path toward agnos and find what kills the sound -----------------
    # Every audit so far COMPARED states and reasoned about which differences "could" matter. That is
    # theory. This is the direct test: take the audibly-working amdgpu path, apply ONE of agnos's values,
    # and listen. The operator's ears are the oracle. Fully reversible; nothing persists across a replug or
    # a mode set.
    #
    # It also directly tests the "upstream is exonerated" logic, which rests on a chain of inference about
    # what the CRC taps observe. A perturbation that kills the sound refutes that chain by demonstration,
    # whatever the taps say.
    for spec in args.reg_write:
        try:
            b_s, o_s, v_s = spec.split(":")
            b, o, v = int(b_s, 0), int(o_s, 0), int(v_s, 0)
        except ValueError:
            print(f"error: --reg-write wants BASE:OFF:VAL, got {spec!r}", file=sys.stderr); sys.exit(2)
        a = addr(b, o)
        old = bar.rd(a)
        bar.wr(a, v)
        print(f"== reg-write BASE_IDX {b} dword 0x{o:04x} (byte 0x{a:06x}) ==")
        print(f"  0x{old:08x} -> wrote 0x{v:08x} -> reads 0x{bar.rd(a):08x}")
        print()
    for spec in args.az_write:
        try:
            e_s, o_s, v_s = spec.split(":")
            ep, ordn, v = int(e_s, 0), int(o_s, 0), int(v_s, 0)
        except ValueError:
            print(f"error: --az-write wants EP:ORD:VAL, got {spec!r}", file=sys.stderr); sys.exit(2)
        idx_a = addr(2, AZ_INDEX0 + ep * AZ_STRIDE)
        dat_a = addr(2, AZ_INDEX0 + ep * AZ_STRIDE + 1)
        saved_idx = bar.rd(idx_a)
        bar.wr(idx_a, ordn)
        old = bar.rd(dat_a)
        bar.wr(dat_a, v)
        bar.wr(idx_a, ordn)          # the window has NO memory — re-select before reading back
        back = bar.rd(dat_a)
        bar.wr(idx_a, saved_idx)     # restore whatever amdgpu had selected, to shrink the race window
        print(f"== az-write endpoint {ep} ordinal 0x{ordn:02x} ==")
        print(f"  0x{old:08x} -> wrote 0x{v:08x} -> reads 0x{back:08x}")
        print()

    # --- The Model A vs Model B discriminator: manufacture "input flowing, output stopped" ---------------
    # AFMT_AUDIO_CRC_SOURCE is ONE bit selecting between two inputs of ONE engine (one enable, one counter,
    # one CRC shift register). DC never reads/writes/references the block, so there is no source ground
    # truth, and two models fit every stimulus measured so far:
    #   Model A: the counter is gated by sample-valid AT THE SELECTED TAP. agnos's tap-1 zeros then mean a
    #            running output stage is emitting silence.
    #   Model B: ONE counter gated by the SHARED INPUT strobe; SOURCE only muxes which data bus feeds the
    #            CRC. agnos's tap-1 zeros then mean "2048 samples ENTERED and the tap-1 bus read zero
    #            throughout" -- i.e. the output stage never fires and samples NEVER LEAVE.
    # tone / digital-silence / nothing-playing score IDENTICALLY under both. The state that separates them
    # is INPUT FLOWING + OUTPUT STOPPED, which never occurs naturally on a working path. This manufactures
    # it, using the encoder's own mute.
    for spec, bit, name in ((args.sample_send, 0, "AFMT_AUDIO_SAMPLE_SEND"),
                            (args.fifo_reset_bit, 11, "AFMT_RESET_FIFO_WHEN_AUDIO_DIS")):
        if spec is None:
            continue
        try:
            dig_s, val_s = spec.split(":")
            dig_n, val = int(dig_s), int(val_s)
        except ValueError:
            print(f"error: --sample-send/--fifo-reset-bit want DIG:VAL, got {spec!r}", file=sys.stderr)
            sys.exit(2)
        a = addr(2, 0x20AA + dig_n * DIG_STRIDE)
        old = bar.rd(a)
        new = (old | (1 << bit)) if val else (old & ~(1 << bit))
        bar.wr(a, new)
        back = bar.rd(a)
        print(f"== {name} on DIG{dig_n} -> {val} ==")
        print(f"  AFMT_AUDIO_PACKET_CONTROL@0x{a:06x}: 0x{old:08x} -> wrote 0x{new:08x} -> reads 0x{back:08x}")
        print()

    if args.crc is not None:
        # THE DISCRIMINATOR. agnos armed exactly this probe on its own path and got CRC_DONE=1 with a
        # non-zero CRC, and concluded "samples physically traverse the AFMT". That conclusion rests on an
        # ASSUMPTION NOBODY VERIFIED: what AFMT_AUDIO_CRC_SOURCE=0 actually taps. It is a 1-bit field
        # (dcn_2_1_0_sh_mask.h: AFMT_AUDIO_CRC_SOURCE__SHIFT 0x8, MASK 0x100) and no AMD documentation says
        # whether 0 is the FIFO input or the packetized output. amdgpu writes this block (dce_v8/v10/v11)
        # and never reads the result back, so nothing validates it.
        #
        # Running it HERE, on a path that is audibly working, pins the meaning:
        #   - amdgpu CRC completes non-zero too => source 0 is a tap both paths reach; agnos's reading stands
        #     as "samples arrive", and the DIFFERENCE between the two CRCs is the signal.
        #   - amdgpu CRC never completes while sound plays => source 0 is NOT a drain-side tap, and agnos's
        #     "samples traverse the block" claim evaporates.
        # Run with --crc-source 0 AND --crc-source 1 and compare: that is what identifies the two taps.
        d = args.crc * DIG_STRIDE
        ctl_a = addr(2, 0x20A2 + d)
        res_a = addr(2, 0x20A8 + d)
        print(f"== AFMT audio CRC probe on DIG{args.crc} "
              f"(count={args.crc_count}, source={args.crc_source}) ==")
        print(f"  CONTROL@0x{ctl_a:06x}  RESULT@0x{res_a:06x}")
        saved_ctl = bar.rd(ctl_a)
        print(f"  amdgpu left CONTROL = 0x{saved_ctl:08x} (expected 0x0 -- amdgpu's DCN path never arms it)")
        bar.wr(ctl_a, 0)
        bar.wr(ctl_a, ((args.crc_count & 0xFFFF) << 16) | ((args.crc_source & 1) << 8) | 0x1)
        done, val = 0, 0
        for _ in range(300):
            time.sleep(0.001)
            val = bar.rd(res_a)
            if val & 0x1:
                done = 1
                break
        bar.wr(ctl_a, saved_ctl)   # put it back exactly as amdgpu had it
        crc = (val >> 8) & 0xFFFFFF
        if done:
            print(f"  RESULT = 0x{val:08x}  ->  CRC_DONE=1  CRC=0x{crc:06x}"
                  f"{'  (ZERO -- samples are digital silence)' if crc == 0 else ''}")
            print(f"  => {args.crc_count} samples traversed the tap selected by SOURCE={args.crc_source} "
                  f"on a path that is AUDIBLY WORKING.")
        else:
            print(f"  RESULT = 0x{val:08x}  ->  CRC_DONE=0 (did not complete in ~300ms)")
            print(f"  => SOURCE={args.crc_source} does NOT complete even while audio is audibly playing.")
            print(f"     If agnos's identical probe DOES complete, agnos is not measuring what it thinks.")
        print()

    bar.close()


if __name__ == "__main__":
    main()
