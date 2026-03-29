# RPi4 Self-Hosted Runner Setup

> Checklist for setting up a Raspberry Pi 4 as a GitHub Actions self-hosted runner
> for aarch64 AGNOS builds (SD card images, arm64 rootfs, cross-validation).

---

## 1. Flash Ubuntu Server to SD Card

On your Mac/Arch workstation:

1. Download: `https://cdimage.ubuntu.com/releases/24.04/release/ubuntu-24.04-preinstalled-server-arm64+raspi.img.xz`
2. Flash with **Balena Etcher** — select the `.img.xz` file (Etcher decompresses automatically), select your SD card, flash

Boot the RPi4, default login: `ubuntu` / `ubuntu` (forces password change on first login).

---

## 2. Initial System Setup (SSH from workstation)

```bash
# From your Mac/Arch box
ssh ubuntu@<rpi-ip>

# Set hostname
sudo hostnamectl set-hostname agnos-arm64

# Update system
sudo apt-get update && sudo apt-get upgrade -y

# Set timezone
sudo timedatectl set-timezone <your-timezone>

# Add your SSH key (from your workstation)
# On workstation: ssh-copy-id ubuntu@<rpi-ip>
```

---

## 3. Install Build Dependencies

These are needed for kernel builds, Rust compilation, and the selfhost-build workflow:

```bash
sudo apt-get install -y \
  build-essential gcc g++ make cmake ninja-build \
  bc flex bison libelf-dev libssl-dev \
  pkg-config autoconf automake libtool \
  curl wget git jq zstd \
  python3 python3-pip \
  debootstrap dosfstools parted e2fsprogs \
  squashfs-tools xorriso mtools \
  qemu-user-static binfmt-support \
  cpio gzip xz-utils \
  libffi-dev zlib1g-dev libncurses-dev \
  uuid-dev libgdbm-dev libreadline-dev
```

---

## 4. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
  --default-toolchain stable \
  --profile default

source ~/.cargo/env

# Verify
rustc --version
cargo --version

# Add targets
rustup target add aarch64-unknown-linux-gnu
```

---

## 5. Expand Rootfs (if SD card is larger than image)

```bash
sudo parted /dev/mmcblk0 resizepart 2 100%
sudo resize2fs /dev/mmcblk0p2
df -h /  # verify full card is available
```

---

## 6. Create Runner User

```bash
sudo useradd -m -s /bin/bash runner
sudo usermod -aG sudo runner
echo "runner ALL=(ALL) NOPASSWD:ALL" | sudo tee /etc/sudoers.d/runner

# Switch to runner user for remaining steps
sudo su - runner
```

---

## 7. Install GitHub Actions Runner

```bash
mkdir ~/actions-runner && cd ~/actions-runner

# Download latest arm64 runner
curl -o actions-runner-linux-arm64.tar.gz -L \
  https://github.com/actions/runner/releases/latest/download/actions-runner-linux-arm64-2.322.0.tar.gz

tar xzf actions-runner-linux-arm64.tar.gz
rm actions-runner-linux-arm64.tar.gz
```

### Register the runner

Go to: `https://github.com/MacCracken/agnosticos/settings/actions/runners/new`

Select **Linux** and **ARM64**. Copy the token, then:

```bash
./config.sh \
  --url https://github.com/MacCracken/agnosticos \
  --token <YOUR_TOKEN> \
  --name agnos-arm64 \
  --labels self-hosted,linux,arm64 \
  --work _work
```

### Install as systemd service

```bash
sudo ./svc.sh install runner
sudo ./svc.sh start
sudo ./svc.sh status
```

---

## 8. Verify Runner is Online

Check: `https://github.com/MacCracken/agnosticos/settings/actions/runners`

Should show `agnos-arm64` with status **Idle**.

---

## 9. Clone Repo (runner workspace)

```bash
# As runner user
cd ~
git clone https://github.com/MacCracken/agnosticos.git
cd agnosticos
cargo check --workspace  # verify Rust builds on arm64
```

---

## 10. Test Kernel Build (optional but recommended)

```bash
curl -fSL -o /tmp/linux-6.6.72.tar.xz \
  https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.6.72.tar.xz
tar xf /tmp/linux-6.6.72.tar.xz -C /tmp
cd /tmp/linux-6.6.72

cp ~/agnosticos/kernel/config/agnos_defconfig .config
# Use edge config for RPi if available:
# cp ~/agnosticos/kernel/config/edge-rpi4.config .config

make olddefconfig
make -j4 Image modules  # arm64 uses Image not bzImage
```

---

## 11. SSH Tunnel from Workstation

For convenience, add to `~/.ssh/config` on your Mac/Arch box:

```
Host rpi
  HostName <rpi-ip>
  User runner
  ForwardAgent yes
```

Then: `ssh rpi`

---

## 12. Re-enable aarch64 Builds

Once the runner is online and verified, in `build-iso.yml` remove the `if: false` lines from:

- `build-aarch64-sdcard`
- `build-aarch64-minimal`
- `build-aarch64-edge`

Update the `runs-on` for those jobs:

```yaml
runs-on: [self-hosted, linux, arm64]
```

---

## Quick Reference

| Item | Value |
|------|-------|
| Runner name | `agnos-arm64` |
| Labels | `self-hosted, linux, arm64` |
| User | `runner` |
| Work dir | `~/actions-runner/_work` |
| SSH | `ssh rpi` from workstation |
| Logs | `sudo journalctl -u actions.runner.*` |
| Restart | `sudo ./svc.sh stop && sudo ./svc.sh start` |

---

*Created: 2026-03-28*
