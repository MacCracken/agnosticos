# AGNOS System Requirements

> Minimum and recommended hardware for running AGNOS across all profiles.
> Last Updated: 2026-09-27
>
> **Pre-Beta note:** AGNOS is not yet installable as an end-user OS. These figures are the **targets** for the first bootable ISO. For what is buildable and testable today, see [README.md](README.md).

---

## Quick Reference

| | Minimum | Recommended |
|---|---------|-------------|
| **CPU** | 64-bit x86_64 or ARM64, 2 cores | 8+ cores, VT-x/AMD-V, AVX2 |
| **RAM** | 4 GB | 8 GB (CLI+LLM), 32 GB (Desktop+LLM) |
| **Storage** | 20 GB SSD | 100 GB NVMe SSD |
| **GPU** | None (headless) | NVIDIA/AMD/Intel discrete or integrated |
| **Boot** | UEFI (GPT + FAT ESP) | UEFI with Secure Boot |
| **Network** | Internet for initial setup | Persistent for cloud/fleet features |

---

## Profiles

AGNOS supports three installation profiles with different hardware floors.

### Server / CLI Profile

Headless operation — daimon (agent runtime), hoosh (LLM gateway), agnoshi (AI shell).
No GPU, no compositor, no desktop packages.

| Component | Minimum | Notes |
|-----------|---------|-------|
| CPU | x86_64 or ARM64, 2 cores | Single-threaded agent ops work on 1 core |
| RAM | 4 GB | 8 GB if running local LLMs via hoosh |
| Storage | 20 GB SSD | Base system ~800 MB; rest for agent data, models, logs |
| GPU | Not required | GPU acceleration optional for hoosh inference |
| Network | Internet for setup + updates | Agents operate locally after install |

**Service memory budgets** (configurable):
- `daimon` (agent orchestrator): 512 MB
- `hoosh` (LLM inference gateway): 8 GB (scales with model size)
- `libro` (audit chain): 128 MB

### Desktop Profile

Full sovereign desktop — the aethersafha compositor (⛔ **not** Wayland; it speaks the sovereign **setu** protocol), browser, creative apps, all CLI services.

| Component | Minimum | Notes |
|-----------|---------|-------|
| CPU | x86_64 or ARM64, 4 cores | VT-x/AMD-V recommended for container isolation |
| RAM | 8 GB | 32 GB DDR5 for local LLMs + creative suite |
| Storage | 40 GB SSD | 100 GB NVMe recommended; LLMs can be 4-70 GB each |
| GPU | OpenGL 3.3+ / Vulkan 1.0+ | Discrete GPU recommended for Tazama/Rasa |
| Display | 1280x720 minimum | 1920x1080+ recommended |
| Audio | ALSA-compatible | PipeWire for DAW (Shruti) usage |
| Network | Internet for setup | Optional after install |

**Supported GPUs**:
- **NVIDIA**: Proprietary driver — version pinned by zugot's `desktops/nvidia-driver.cyml`. Its 595-series pin supports **Turing and newer (GTX 16 / RTX 20+)**: NVIDIA's 580 branch was the last for Maxwell, Pascal and Volta, and the 470 branch the last for Kepler
- **AMD**: Mesa radeonsi (GCN 1.0 and newer, HD 7000+)
- **Intel**: Mesa iris/i965 (Broadwell and newer, HD 5500+)
- **Software rendering**: Mesa swrast (llvmpipe) — functional but slow

**Sovereign display path** (AGNOS kernel — no Mesa, no amdgpu): the kernel drives AMD Cezanne (gfx90c / DCN 2.1) directly via a sovereign ATOM BIOS interpreter plus kernel-side DCN modeset, **iron-confirmed at native 2560x1440 with the scaler in bypass** on archaemenid (agnos 1.56.36–1.56.38, burned PASS). ⚠️ That is one GPU family on one box — the Mesa rows above still describe the Linux-host bootstrap path, and no other silicon has been burned. Current status: [`../development/state.md`](../development/state.md).

**Desktop memory budget**: 2 GB for aethersafha compositor.

### Edge / IoT Profile

Minimal footprint for embedded devices — fleet management, OTA updates, telemetry.

| Component | Minimum | Notes |
|-----------|---------|-------|
| CPU | ARM64 (RPi4/5) or x86_64 (NUC) | Single-core capable |
| RAM | 256 MB | 1 GB recommended; edge agent uses 64 MB max |
| Storage | 256 MB | Read-only rootfs with overlay; SD card or eMMC |
| GPU | Not required | Headless operation |
| Network | Required | Fleet management, OTA, telemetry |
| TPM | Optional | 2.0 recommended for device attestation |

**Edge-specific limits**:
- AGNOS Edge Agent: 64 MB RAM max
- SecureYeoman Edge: 32 MB RAM max
- OTLP telemetry buffer: 8 MB max
- Kernel: Minimal 6.6 LTS edge defconfig

**Tested hardware**:
- Raspberry Pi 4 (BCM2711, 1-4 GB)
- Raspberry Pi 5
- Intel NUC (various generations)

---

## Architecture Support

| Architecture | Status | Notes |
|--------------|--------|-------|
| x86_64 (AMD64) | Full support | Primary development target; Cyrius self-hosts byte-identical |
| ARM64 (AArch64) | Toolchain full; kernel does not compile | Cyrius cross-compiler + native Pi self-host byte-identical (v5.3.15+). The AGNOS kernel's aarch64 tree does not currently compile (found at agnos 1.56.51, 2026-08-28, when `test.sh`'s silently-skipping aarch64 probe was fixed; `test.sh --all` is red on it) and has no boot harness |
| Apple Silicon (Mach-O) | Compiler toolchain only | Cyrius self-hosts byte-identically on M-series (v5.3.13); AGNOS kernel targets Linux ABI |
| Windows PE32+ | Compiler toolchain only | Cyrius native self-host byte-identical on real Windows 11 (v5.5.10) |
| RISC-V (rv64) | Queued | Cyrius **v6.7.x / v6.8.x** — re-scheduled off the v6.0.x line; not shipped and not imminent (`cyrius/docs/development/roadmap.md`) |
| Bare-metal (no host OS) | Toolchain target shipped | Cyrius **v6.2.x** (bare-metal target formalized at the v6.2.52 close); public-beta self-hosting scope, not a closed-beta gate |
| x86 (32-bit) | Not supported | No kernel configs, no recipes |
| ARM (32-bit) | Not supported | Out of scope |

---

## Firmware & Security Hardware

| Feature | Required? | Notes |
|---------|-----------|-------|
| UEFI | Required | gnoboot (sovereign UEFI PE32+ EFI Application; version in its `VERSION`) is the default bootloader, on GPT + FAT ESP; since 0.6.1 it selects the largest RGB/BGR GOP mode the firmware offers rather than inheriting whatever mode was left |
| Legacy BIOS | Not supported | GRUB / multiboot2 path retired; boot is UEFI-only via gnoboot |
| Secure Boot | Optional — not yet on the gnoboot path | gnoboot is not Secure-Boot-signed and does not verify the kernel's signature; a signing chain is a post-v1.0 gnoboot direction. (This row used to claim full MOK enrollment support — no current boot component provides it.) |
| TPM 2.0 | Optional | Enables disk encryption key sealing, measured boot, device attestation |
| LUKS disk encryption | Optional | LUKS2 via cryptsetup; works with or without TPM |
| dm-verity | Optional | Verified root filesystem; used by Edge profile |
| IMA | Optional | Integrity Measurement Architecture for file integrity |

---

## Kernel

AGNOS uses **two kernels** depending on profile — the AGNOS kernel is primary; a Linux kernel is retained for host bootstrap and driver coverage during the transition to full sovereignty.

### AGNOS Kernel (sovereign, primary)

- **Version**: Cyrius-native, with a small sovereign syscall surface — no BSD `socket` call, no `splice`, no AF_ALG family. (AGNOS *does* have a kernel TCP/UDP/ICMP stack; its `sock_*` / `udp_*` calls are purpose-built and take no address-family argument, so there is no family to select.) Head version, module count and build size drift every cut — see [`../development/state.md`](../development/state.md) rather than a figure quoted here
- **Iron-validated on AMD**: exec-from-disk, SMP, shell-separation and >256 MB RAM support; HDA analog audio (1.52.x); kernel FP/SIMD (1.53.x); GPU compute on the Cezanne iGPU with no amdgpu and no ROCm (1.54.x); display scanout, the sovereign ATOM BIOS interpreter and ACPI S5 self-poweroff (1.55.x); GPU 3D raster, kernel-side DCN modeset and native-resolution scanout (1.56.0–1.56.38). ⚠️ 1.56.39 is QEMU-only and was never burned. As of 2026-09-27 the last iron burn is **1.57.1** (2026-09-08); **1.57.2 → 1.57.10 are built and gated but not burned** — per-cut status in [`../development/state.md`](../development/state.md)
- **Repo**: `MacCracken/agnos`
- **Multi-arch split** (v1.1.0): `kernel/arch/x86_64/`, `kernel/arch/aarch64/`, `kernel/core/`, `kernel/user/`. ⚠️ Only x86_64 boots — the aarch64 tree **does not currently compile** (found at 1.56.51) and has no boot harness
- **Boot**: gnoboot (sovereign UEFI PE32+ EFI Application) from a GPT + FAT ESP; boots in QEMU through gnoboot + OVMF (`gnoboot/tests/ovmf_smoke.sh`). The genesis repo's `make boot-test` uses QEMU `-kernel`, which cannot load the kernel since agnos 1.30.0

### Linux Kernel (host bootstrap, transitional)

- **Version**: Linux 6.6 LTS — patch level pinned by zugot's `base/linux.cyml`
- **Role**: used during early ISO builds for broad driver coverage while sovereign stack gaps close
- **Security**: Landlock, seccomp, AppArmor/SELinux, kernel lockdown LSM
- **Filesystems**: ext4, btrfs, xfs, vfat, squashfs, overlayfs, FUSE
- **Networking**: namespaces, nftables, WireGuard, bridging
- **Hardware**: USB 3.x (xHCI), Thunderbolt/USB4 (boltd), NVMe, SATA, SD/MMC

---

## Peripheral Support

| Peripheral | Package | Notes |
|------------|---------|-------|
| WiFi | linux-firmware + kernel drivers | Intel (iwlwifi), Broadcom (brcmfmac), Atheros, Realtek |
| Bluetooth | BlueZ | BLE + mesh; MIDI support |
| Thunderbolt/USB4 | boltd | TB3/TB4 authorization and security |
| Printing | CUPS | Optional; web admin interface |
| Audio | ALSA + PipeWire | Intel HDA, USB audio |
| Webcam/V4L2 | Kernel V4L2 | USB and built-in cameras |

Package versions are pinned by the zugot recipes, not here.

---

## "How Far Back Can You Go?"

The oldest hardware that can run AGNOS, by profile:

### Desktop (oldest viable)
- **CPU**: Intel Broadwell (2014) / AMD GCN 1.0 (2012) — for GPU driver support
- **GPU**: NVIDIA RTX 20 / GTX 16 series (Turing, 2018 — the floor of the pinned 595-series driver) / AMD HD 7000 (2012) / Intel HD 5500 (2015)
- **RAM**: 8 GB DDR3 is functional, DDR4 recommended
- **Motherboard**: Any x86_64 with UEFI (most boards since ~2012)
- **Practical floor**: ~2014-2015 era hardware

### Server/CLI (oldest viable)
- **CPU**: Any 64-bit x86_64 (Intel Core 2 / AMD Athlon 64, ~2006) or ARM64
- **RAM**: 4 GB DDR2/DDR3 is functional
- **Motherboard**: UEFI required (gnoboot is UEFI-only; no legacy-BIOS boot path)
- **Practical floor**: ~2010 era hardware (4 GB RAM was common by then)

### Edge (oldest viable)
- **Board**: Raspberry Pi 4 (2019) or any ARM64 SBC with 256 MB+ RAM
- **x86_64**: Intel Atom or Celeron NUC (any generation with 64-bit)
- **Practical floor**: ~2019 for ARM64 SBCs, ~2012 for x86_64 embedded

---

## Software Dependencies (Host Build)

Building AGNOS from source requires:
- **Cyrius toolchain** — version pinned in `scripts/cyrius.cyml` (the `cyrius = "<version>"` field); install from release tarball or build from source (`cyrius` repo)
- **QEMU** — `qemu-system-x86_64` (≥ 7.0), plus OVMF, parted and mtools, to boot the kernel through gnoboot (`make boot-test`'s `-kernel` path cannot load it since agnos 1.30.0)
- **GNU Make** — `make boot`, `make iso-check`, `make boot-test`

The Rust / GCC / Python / Docker toolchain was retired during the Cyrius pivot (2026-04-04). Pre-Cyrius scripts live in `scripts/archive-pre-cyrius/` for reference only.

---

*For installation instructions, see [README.md](README.md). For development setup, see [CONTRIBUTING.md](../../CONTRIBUTING.md).*
