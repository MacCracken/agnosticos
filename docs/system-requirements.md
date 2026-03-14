# AGNOS System Requirements

> Minimum and recommended hardware for running AGNOS across all profiles.
> Last Updated: 2026-03-14

---

## Quick Reference

| | Minimum | Recommended |
|---|---------|-------------|
| **CPU** | 64-bit x86_64 or ARM64, 2 cores | 8+ cores, VT-x/AMD-V, AVX2 |
| **RAM** | 4 GB | 8 GB (CLI+LLM), 32 GB (Desktop+LLM) |
| **Storage** | 20 GB SSD | 100 GB NVMe SSD |
| **GPU** | None (headless) | NVIDIA/AMD/Intel discrete or integrated |
| **Boot** | UEFI or Legacy BIOS | UEFI with Secure Boot |
| **Network** | Internet for initial setup | Persistent for cloud/fleet features |

---

## Profiles

AGNOS supports three installation profiles with different hardware floors.

### Server / CLI Profile

Headless operation — daimon (agent runtime), hoosh (LLM gateway), agnoshi (AI shell).
No GPU, no Wayland, no desktop packages.

| Component | Minimum | Notes |
|-----------|---------|-------|
| CPU | x86_64 or ARM64, 2 cores | Single-threaded agent ops work on 1 core |
| RAM | 4 GB | 8 GB if running local LLMs via hoosh |
| Storage | 20 GB SSD | Base system ~800 MB; rest for agent data, models, logs |
| GPU | Not required | GPU acceleration optional for hoosh inference |
| Network | Internet for setup + updates | Agents operate locally after install |

**Service memory budgets** (configurable):
- `agent-runtime` (daimon): 512 MB
- `llm-gateway` (hoosh): 8 GB (scales with model size)
- `agnos-audit`: 128 MB

### Desktop Profile

Full Wayland desktop — aethersafha compositor, browser, creative apps, all CLI services.

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
- **NVIDIA**: Proprietary driver 570.x (Kepler and newer, GTX 600+)
- **AMD**: Mesa radeonsi (GCN 1.0 and newer, HD 7000+)
- **Intel**: Mesa iris/i965 (Broadwell and newer, HD 5500+)
- **Software rendering**: Mesa swrast (llvmpipe) — functional but slow

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
| x86_64 (AMD64) | Full support | Primary development target |
| ARM64 (AArch64) | Full support | Cross-compilation via Cross.toml |
| x86 (32-bit) | Not supported | No kernel configs, no recipes |
| ARM (32-bit) | Not supported | Cross-compilation stubs exist but untested |
| RISC-V | Not supported | Future consideration |

---

## Firmware & Security Hardware

| Feature | Required? | Notes |
|---------|-----------|-------|
| UEFI | Recommended | systemd-boot is the default bootloader |
| Legacy BIOS | Supported | Via GRUB 2 |
| Secure Boot | Optional | Full MOK enrollment support; recommended for production |
| TPM 2.0 | Optional | Enables disk encryption key sealing, measured boot, device attestation |
| LUKS disk encryption | Optional | LUKS2 via cryptsetup 2.8.1; works with or without TPM |
| dm-verity | Optional | Verified root filesystem; used by Edge profile |
| IMA | Optional | Integrity Measurement Architecture for file integrity |

---

## Kernel

- **Version**: Linux 6.6 LTS (6.6.80)
- **Security**: Landlock, seccomp, AppArmor/SELinux, kernel lockdown LSM
- **Filesystems**: ext4, btrfs, xfs, vfat, squashfs, overlayfs, FUSE
- **Networking**: namespaces, nftables, WireGuard, bridging
- **Hardware**: USB 3.x (xHCI), Thunderbolt/USB4 (boltd), NVMe, SATA, SD/MMC

---

## Peripheral Support

| Peripheral | Package | Notes |
|------------|---------|-------|
| WiFi | linux-firmware + kernel drivers | Intel (iwlwifi), Broadcom (brcmfmac), Atheros, Realtek |
| Bluetooth | BlueZ 5.82 | BLE + mesh; MIDI support |
| Thunderbolt/USB4 | boltd 0.9.8 | TB3/TB4 authorization and security |
| Printing | CUPS 2.4.12 | Optional; web admin interface |
| Audio | ALSA + PipeWire | Intel HDA, USB audio |
| Webcam/V4L2 | Kernel V4L2 | USB and built-in cameras |

---

## "How Far Back Can You Go?"

The oldest hardware that can run AGNOS, by profile:

### Desktop (oldest viable)
- **CPU**: Intel Broadwell (2014) / AMD GCN 1.0 (2012) — for GPU driver support
- **GPU**: NVIDIA GTX 600 series (2012) / AMD HD 7000 (2012) / Intel HD 5500 (2015)
- **RAM**: 8 GB DDR3 is functional, DDR4 recommended
- **Motherboard**: Any x86_64 with UEFI (most boards since ~2012)
- **Practical floor**: ~2014-2015 era hardware

### Server/CLI (oldest viable)
- **CPU**: Any 64-bit x86_64 (Intel Core 2 / AMD Athlon 64, ~2006) or ARM64
- **RAM**: 4 GB DDR2/DDR3 is functional
- **Motherboard**: BIOS or UEFI (GRUB 2 handles legacy BIOS)
- **Practical floor**: ~2010 era hardware (4 GB RAM was common by then)

### Edge (oldest viable)
- **Board**: Raspberry Pi 4 (2019) or any ARM64 SBC with 256 MB+ RAM
- **x86_64**: Intel Atom or Celeron NUC (any generation with 64-bit)
- **Practical floor**: ~2019 for ARM64 SBCs, ~2012 for x86_64 embedded

---

## Software Dependencies (Host Build)

Building AGNOS from source requires:
- Rust toolchain (stable, latest)
- GCC 12+ or Clang 15+
- GNU Make, CMake, Meson, Ninja
- OpenSSL 3.x headers
- SQLite 3.x headers
- pkg-config
- Python 3.10+ (for build scripts)
- Docker/Podman (for container builds and ISO generation)

---

*For installation instructions, see [docs/installation/](installation/). For development setup, see [CONTRIBUTING.md](/CONTRIBUTING.md).*
