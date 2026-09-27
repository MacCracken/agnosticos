# AGNOS Firmware-Provenance Ledger

> **Purpose.** A precise, honest record of every piece of **opaque vendor firmware** AGNOS ships or
> loads, so the sovereignty claim stays exact rather than aspirational. Policy + rationale:
> [[project_firmware_blob_posture]] (memory). Standing posture (user, 2026-07-11): accept
> vendor-signed firmware **per-manufacturer** as hardware-required (the CPU-microcode category),
> **until a manufacturer authorizes AGNOS to write + sign its own** — a partnership path. The
> sovereignty win lives **above** the microcode (no vendor driver stack — agnos drives the silicon
> itself); the microcode itself is the price of the hardware.
>
> **The claim we stake, precisely:** *"AGNOS is sovereign except the vendor microcode — it ships no
> vendor driver stack (no ROCm, no Mesa/RADV, no Vulkan loader); the only opaque element is on-chip
> microcode that no one — not Linux, not Mesa, not tinygrad — can replace on this silicon."* Never
> claim "blob-free GPU compute" on signed-firmware parts; it is false.

---

## GPU — AMD Cezanne iGPU (archaemenid, `1002:1638`, gfx90c/Renoir)

**Decision (user, 2026-07-11): ship the blobs + a display-only max-sovereignty tier.** GPU *compute*
requires the on-die PSP to load AMD-signed CP/MEC/RLC microcode; the PSP hard-rejects anything not
AMD-signed, so self-built ucode is impossible on this silicon (for everyone — Linux/Mesa/tinygrad
included). Iron-confirmed at agnos 1.54.1 (C0 = CASE B — ucode not resident; the OS must PSP-load).

**Compute subset AGNOS loads (the 1.54.x GPU arc, C1 firmware-load).** From linux-firmware
`amdgpu/` (Cezanne = codename `green_sardine`, symlinked to `renoir_*.bin`; the gfx90c/Vega ISA is
shared across Renoir/Lucienne/Cezanne/Barcelo, so `renoir_*` is the reference set):

| Blob | Uncompressed | Role | Required for |
|------|-------------:|------|--------------|
| `renoir_gpu_info.bin` | 316 B | ASIC config table (engine/CU/queue counts) | ASIC init (all modes) |
| `renoir_rlc.bin` | 39,928 B | RLC — power/clock gating + CP init sequencing | compute + gfx |
| `renoir_mec.bin` | 268,224 B | MEC1 compute microengine (HQD/MQD queues) | **compute (the crown)** |
| `renoir_mec2.bin` | 268,224 B | MEC2 (2nd compute pipe) | full compute (MEC1-only MVP can skip) |
| `renoir_ce.bin` | 9,344 B | CP Constant Engine | GFX ring (2D/3D — Thrust P) |
| `renoir_pfp.bin` | 21,632 B | CP Prefetch Parser | GFX ring |
| `renoir_me.bin` | 17,536 B | CP Micro Engine | GFX ring |
| `renoir_sdma.bin` | 17,408 B | SDMA copy engine | GPUVM staging / data path |

**Compute subset total ≈ 628 KB** (RLC + MEC1 minimal ≈ 308 KB). Not shipped for compute:
`renoir_dmcub.bin` (121.6 KB, display modeset — Thrust P only; a GOP-lit static framebuffer needs
**zero** GPU ucode), `renoir_asd.bin` / `renoir_ta.bin` (PSP trusted-apps — HDCP/DTM, optional),
`renoir_vcn.bin` (404.5 KB, HW video — optional). **No GPU PSP bootloader/SOS blob** — APUs don't
ship one; the PSP itself lives in the platform BIOS/AGESA/SPI and runs before agnos.

**Provenance + license.** Source: linux-firmware (`gitlab.com/kernel-firmware/linux-firmware`,
`amdgpu/`) or AMD's `github.com/amd/firmware_binaries`. License: **`LICENSE.amdgpu`** — binary
redistribution **permitted** (reproduce AMD's notice with the distribution; don't use AMD's name to
promote derivatives); reverse-engineering, decompilation, disassembly **expressly prohibited**.
GPL-incompatible (distro-firmware class — same as CPU `amd-ucode` and most NIC firmware). AGNOS may
legally **ship** these binaries; it may **not** legally RE them into a sovereign replacement.

**Tiers shipped.** (1) **Compute/desktop tier** — bundle the ~628 KB compute subset, PSP-loaded at
GPU init. (`dmcub` was expected to join when real modeset landed; it did not. The GOP leaves DMCUB
dormant, DMCUB-mediated bring-up was retired at agnos 1.55.22 (2026-07-18), and the kernel-side DCN
modeset that followed — the sovereign ATOM interpreter plus direct DCN programming — loads no DMCUB
firmware.) (2) **Display-only max-sovereignty tier** — zero GPU
ucode, DCN pipe lit from the gnoboot/GOP linear framebuffer; forfeits compute + accel, offered as an
explicit maximum-sovereignty mode, not a substitute.

**Staging (DONE, C1b-2 + C1c):** the subset is decompressed (`zstd -d`) from the host linux-firmware
`amdgpu/` (Cezanne = `green_sardine`, symlinked from `renoir_*`) and staged into the agnos-fs rootfs
at `build/rootfs/fw/` under short names — `rlc.bin` (C1b-2, 39,928 B), `ce.bin` (9,344), `pfp.bin`
(21,632), `me.bin` (17,536), `mec.bin` (268,224) (C1c) — reaching the box at `/fw/` via
`install-media.sh --update-all` (the `on-disk /fw` echo lists them). The kernel (`gpu_fw_load` +
`gpu_fw_load_set`) reads each blob and feeds it to the PSP via `LOAD_IP_FW`; the PSP validates AMD's
signature. **`mec2` is NOT staged/loaded** — `renoir_mec2.bin` symlinks to the identical
`green_sardine_mec.bin`, and gfx9.3.0 loads no separate MEC2 (the single MEC ucode serves both
pipes). `sdma.bin` has since been staged too, and was PSP-loaded on iron at agnos 1.55.29
(2026-07-21) — the SDMA engine itself then stayed parked on an agnos setup gap. RLC restore-lists
(GFXOFF) and `dmcub` are not staged. The distributed
image must carry AMD's `LICENSE.amdgpu` notice text.

---

## CPU — AMD Ryzen 7 5800H microcode

Not loaded by AGNOS — the platform BIOS/AGESA + the on-die PSP apply CPU microcode before agnos
boots (the standard for AMD). Listed here for completeness of the "opaque vendor firmware AGNOS
depends on" picture: it is exactly the same category as the GPU microcode above, and it predates and
underlies everything AGNOS does on this machine.

---

## Other device firmware

- **NIC (r8169 / RTL8125)** — no separate firmware blob loaded by AGNOS's sovereign driver (the
  chip runs its resident firmware). Revisit if a firmware-requiring NIC is added.
- **NVMe / SATA controllers** — run vendor-resident controller firmware; AGNOS loads none.

*Ledger opened 2026-07-11 alongside the GPU-compute decision. Update whenever AGNOS ships or loads a
new opaque-firmware dependency; keep sizes + license current.*
