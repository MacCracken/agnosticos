# AGNOS Installation Guide

> **Version:** 2026.3.12 | **Last Updated:** 2026-03-13

This guide covers installing AGNOS on bare metal hardware and virtual machines.

---

## Requirements

### Hardware

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | x86_64, 2 cores | x86_64, 4+ cores |
| RAM | 2 GB | 8 GB (16 GB for AI workloads) |
| Disk | 20 GB | 100 GB SSD |
| Boot | UEFI or Legacy BIOS | UEFI with Secure Boot |
| GPU | — | NVIDIA, AMD, or Intel (for desktop) |

### Supported Hardware

- **GPU**: NVIDIA (proprietary 570.x or nouveau), AMD (radeonsi), Intel (iris)
- **WiFi**: Broadcom, Intel, Atheros, Realtek (via linux-firmware)
- **Bluetooth**: BlueZ 5.82 (BLE + mesh)
- **Thunderbolt**: USB4/TB3/TB4 via boltd
- **Printing**: CUPS 2.4 (optional)

---

## Installation Methods

### Method 1: ISO Install (Recommended)

#### Step 1 — Download the ISO

```bash
# From the releases page or build locally:
make iso

# Output: build/agnos-VERSION-x86_64.iso
```

#### Step 2 — Create bootable media

**USB drive:**
```bash
# WARNING: This erases the target device
sudo dd if=build/agnos-VERSION-x86_64.iso of=/dev/sdX bs=4M status=progress
sync
```

**Or use a GUI tool:** Balena Etcher, Ventoy, or Rufus (Windows).

#### Step 3 — Boot from USB/ISO

1. Insert the USB drive (or mount the ISO in your VM)
2. Enter BIOS/UEFI setup (usually F2, F12, DEL, or ESC at POST)
3. Set boot order to prioritize USB/optical
4. Save and reboot

#### Step 4 — AGNOS Installer (agnova)

The installer runs automatically on first boot. It will walk you through:

**Disk Configuration:**
```
Select installation disk:
  [1] /dev/sda  Samsung 870 EVO 500GB
  [2] /dev/nvme0n1  WD Black SN850X 1TB

Partition scheme:  GPT (recommended) / MBR
Filesystem:        ext4 / btrfs / xfs
Encryption:        LUKS2 (optional, recommended)
```

**Installation Mode:**
| Mode | Description |
|------|-------------|
| **Desktop** | Full desktop environment (aethersafha compositor, GPU drivers, audio, printing) |
| **Server** | Headless, agent runtime + LLM gateway, no GUI |
| **Minimal** | Base system only, build from there |
| **Custom** | Choose individual packages |

**User Setup:**
- Root password
- Initial user account (added to `wheel` group for shakti/sudo)
- Hostname and timezone

**Bootloader:**
- systemd-boot (UEFI, default) or GRUB 2 (UEFI/BIOS)
- Automatic kernel parameter configuration

#### Step 5 — First Boot

After installation completes and reboot:

1. **Argonaut init** starts services in dependency order
2. **daimon** (agent runtime) starts on port 8090
3. **hoosh** (LLM gateway) starts on port 8088
4. Login at the console or desktop

Verify the system:
```bash
# Check version
cat /etc/agnos/version

# Check services
ark status

# Verify agent runtime
curl -s http://127.0.0.1:8090/v1/health

# Verify LLM gateway
curl -s http://127.0.0.1:8088/v1/health

# Launch AI shell
agnsh
```

---

### Method 2: QEMU/KVM Virtual Machine

#### Quick Start (graphical)

```bash
# Build the ISO first
make iso

# Boot in QEMU with GUI display
sudo qemu-system-x86_64 \
  -m 2G -smp 2 -enable-kvm \
  -cdrom output/agnos-*-x86_64.iso \
  -boot d \
  -nic user,hostfwd=tcp::2222-:22,hostfwd=tcp::18090-:8090,hostfwd=tcp::18088-:8088

# Or use the automated boot test:
./scripts/qemu-boot-test.sh --disk 20G
```

#### Headless / SSH (no display)

If you're connected via SSH or have no graphical environment:

```bash
# Serial console mode — boots directly in your terminal
sudo qemu-system-x86_64 \
  -m 2G -smp 2 -enable-kvm \
  -cdrom output/agnos-*-x86_64.iso \
  -boot d \
  -nographic \
  -nic user,hostfwd=tcp::2222-:22,hostfwd=tcp::18090-:8090,hostfwd=tcp::18088-:8088
```

Select the **"Serial Console"** entry from the GRUB menu for proper serial output.
To exit QEMU: `Ctrl-a x`.

Alternatively, use VNC:
```bash
sudo qemu-system-x86_64 \
  -m 2G -smp 2 -enable-kvm \
  -cdrom output/agnos-*-x86_64.iso \
  -boot d \
  -vnc :0 \
  -nic user,hostfwd=tcp::2222-:22,hostfwd=tcp::18090-:8090,hostfwd=tcp::18088-:8088
```
Then connect a VNC client to `localhost:5900`.

#### Persistent disk install

```bash
# Create a disk image for persistent installs
qemu-img create -f qcow2 agnos-disk.qcow2 40G

# Boot with ISO + disk
sudo qemu-system-x86_64 \
  -m 4G -smp 4 -enable-kvm \
  -cdrom output/agnos-*-x86_64.iso \
  -boot d \
  -drive file=agnos-disk.qcow2,format=qcow2,if=virtio \
  -nic user,hostfwd=tcp::2222-:22,hostfwd=tcp::18090-:8090,hostfwd=tcp::18088-:8088
```

#### UEFI Boot (recommended)

Install OVMF firmware:
```bash
# Debian/Ubuntu
sudo apt install ovmf

# Arch
sudo pacman -S edk2-ovmf

# Fedora
sudo dnf install edk2-ovmf
```

Add to QEMU command:
```bash
-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.fd
```

#### Port Forwarding

AGNOS services are forwarded to non-conflicting host ports:

| Service | Guest Port | Host Port |
|---------|-----------|-----------|
| SSH | 22 | 2222 |
| daimon (agent runtime) | 8090 | 18090 |
| hoosh (LLM gateway) | 8088 | 18088 |

From the host:
```bash
ssh -p 2222 user@localhost             # password: agnos
curl http://localhost:18090/v1/health  # agent runtime
curl http://localhost:18088/v1/models  # LLM gateway
```

---

### Method 3: Docker Container

For evaluation and development without a full install:

```bash
# Pull the pre-built image
docker pull ghcr.io/maccracken/agnosticos:latest

# Run with AI shell
docker run -it \
  -p 8088:8088 \
  -p 8090:8090 \
  ghcr.io/maccracken/agnosticos:latest \
  agnsh

# Run as daemon
docker run -d \
  --name agnos \
  -p 8088:8088 \
  -p 8090:8090 \
  -v agnos-data:/var/lib/agnos \
  ghcr.io/maccracken/agnosticos:latest
```

#### With GPU Support (NVIDIA)

```bash
docker run -d \
  --gpus all \
  --name agnos-gpu \
  -p 8088:8088 \
  -p 8090:8090 \
  ghcr.io/maccracken/agnosticos:latest
```

#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AGNOS_ULIMIT_VMEM` | `8388608` | Virtual memory limit (KB), `unlimited` to disable |
| `AGNOS_ULIMIT_NOFILE` | `4096` | Max open file descriptors |
| `AGNOS_ULIMIT_NPROC` | `256` | Max user processes |
| `OLLAMA_HOST` | `http://host.docker.internal:11434` | Ollama endpoint |
| `OPENAI_API_KEY` | — | OpenAI API key for cloud inference |
| `ANTHROPIC_API_KEY` | — | Anthropic API key for cloud inference |

---

### Method 4: Build from Source

#### Prerequisites

```bash
# Install build dependencies (Debian/Ubuntu)
./scripts/install-build-deps.sh

# Or manually (Debian/Ubuntu):
sudo apt install build-essential gcc g++ make cmake ninja-build \
  autoconf automake libtool pkg-config bison flex gawk m4 \
  texinfo bc kmod libssl-dev libseccomp-dev libcap-dev \
  curl wget rsync qemu-system-x86 qemu-utils \
  debootstrap squashfs-tools grub-common grub-pc-bin xorriso mtools musl-tools

# Or manually (Arch Linux):
sudo pacman -S base-devel cmake ninja squashfs-tools grub libisoburn mtools \
  qemu-full debootstrap debian-archive-keyring musl
```

#### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Add musl target for static binaries (required for ISO builds)
rustup target add x86_64-unknown-linux-musl
```

#### Build Everything

```bash
# Clone
git clone https://github.com/agnostos/agnos.git
cd agnos

# Build userland (static musl binaries for ISO portability)
cargo build --release --target x86_64-unknown-linux-musl \
  --manifest-path userland/Cargo.toml

# Build kernel
./scripts/build-kernel.sh -v 6.6-lts

# Build base system packages
./scripts/ark-build-all.sh recipes/base/

# Build ISO
make iso
```

#### Building the ISO

The ISO builder (`scripts/build-iso.sh`) creates a bootable live image using a Debian
base with AGNOS userland installed on top. It must be run as root.

```bash
# Full build (compiles userland + bootstraps rootfs + creates ISO)
sudo ./scripts/build-iso.sh

# Skip cargo build (use pre-built musl binaries)
sudo ./scripts/build-iso.sh --skip-build

# Skip debootstrap (reuse existing rootfs, just update binaries + repackage)
sudo ./scripts/build-iso.sh --skip-build --skip-debootstrap
```

**Static linking (musl):** The ISO builder compiles with `--target x86_64-unknown-linux-musl`
to produce fully static binaries. This avoids glibc version mismatches between the build
host and the Debian-based ISO rootfs. If you build manually, always use the musl target
for ISO-destined binaries.

**Output:** `output/agnos-VERSION-x86_64.iso` (~350 MB)

The ISO includes:
- Debian minimal base (systemd, networking, SSH)
- AGNOS binaries: `agent-runtime`, `llm-gateway`, `agnsh`, `agnos-sudo`
- Systemd units for daimon (agent runtime) and hoosh (LLM gateway)
- AGNOS init scripts and sysctl hardening
- GRUB bootloader with 5 boot modes (normal, live, debug, serial console, recovery)
- Default login: `user`/`agnos` or `root`/`agnos`

#### Self-Hosting Validation

After building, verify the system can rebuild itself:

```bash
# Quick check (compile tests only)
make selfhost-validate-quick

# Full validation (builds packages, compiles kernel modules)
make selfhost-validate

# QEMU boot test
make qemu-boot-test
```

---

## Post-Install Configuration

### Connect to LLM Providers

Edit `/etc/agnos/hoosh.toml` or set environment variables:

```bash
# Local (Ollama — auto-detected if running)
export OLLAMA_HOST=http://localhost:11434

# Cloud providers
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-ant-...
export GOOGLE_API_KEY=...
```

Verify:
```bash
curl http://127.0.0.1:8088/v1/models
```

### Install Marketplace Apps

```bash
# Search available apps
agnsh marketplace search

# Install an app
agnsh marketplace install secureyeoman

# List installed
agnsh marketplace list
```

### Configure Desktop Environment

For desktop installations, the aethersafha compositor starts automatically. Configure via:

```bash
# Theme
/etc/agnos/desktop/theme.toml

# Display settings
/etc/agnos/desktop/display.toml

# Keyboard/input
/etc/agnos/desktop/input.toml
```

### Hardening

AGNOS comes hardened by default:
- Landlock LSM for filesystem sandboxing
- seccomp-BPF for syscall filtering
- Cryptographic audit chain in `/var/log/agnos/audit.log`
- mTLS for service-to-service communication
- Network namespace isolation per agent

Review and customize:
```bash
# Security posture
/etc/agnos/security/aegis.toml

# Sandbox profiles
/etc/agnos/sandbox/profiles/

# CIS benchmark compliance
cat /etc/agnos/security/cis-report.txt
```

---

## Troubleshooting

### Boot Issues

| Symptom | Solution |
|---------|----------|
| Black screen after GRUB | Add `nomodeset` to kernel parameters |
| Kernel panic on boot | Check RAM, try `memtest86+` from boot menu |
| No network on boot | Check `ip link`, WiFi needs `linux-firmware` package |
| Services not starting | Check `journalctl -u argonaut` or `/var/log/agnos/boot.log` |

### Service Issues

```bash
# Check agent runtime
curl -v http://127.0.0.1:8090/v1/health

# Check LLM gateway
curl -v http://127.0.0.1:8088/v1/health

# View logs
tail -f /var/log/agnos/agent-runtime.log
tail -f /var/log/agnos/llm-gateway.log

# Restart services
# (via argonaut)
ark service restart agent-runtime
```

### Getting Help

- **Issue Tracker**: https://github.com/agnostos/agnos/issues
- **Security Issues**: Report privately (see `SECURITY.md`)
- **Documentation**: https://docs.agnos.org

---

*See also: [CONTRIBUTING.md](/CONTRIBUTING.md) for development setup, [API Reference](/docs/api/README.md) for endpoint documentation.*
