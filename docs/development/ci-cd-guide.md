# CI/CD & Build Infrastructure Guide

> **Last Updated**: 2026-03-16

## Overview

AGNOS uses GitHub Actions for continuous integration and delivery. The pipeline
produces tested binaries, bootable ISOs, cross-toolchains, and container images.

```
Push to main
  │
  ├─→ ci.yml              — cargo test, clippy, fmt (every push)
  ├─→ build-iso.yml        — Debian-based ISO (every push, fast)
  │
  ├─→ selfhost-build.yml   — Bootstrap toolchain → base system → ISO (on recipe/script changes)
  │     ├── Job 1: bootstrap-toolchain  → artifact: agnos-toolchain-x86_64
  │     ├── Job 2: build-base           → artifact: agnos-rootfs-x86_64
  │     └── Job 3: create-iso           → artifact: agnos-iso-x86_64
  │
  ├─→ selfhost-validate.yml — Boot ISO in QEMU → run validation (after selfhost-build)
  │
  └─→ publish-toolchain.yml — Publish toolchain as release + container (after selfhost-build)
        ├── GitHub Release: toolchain-latest tag
        └── Container: ghcr.io/maccracken/agnos-toolchain:latest
```

## Workflows

### ci.yml — Standard CI

**Triggers**: Every push to `main`, every PR.

**Jobs**:
- `check` — `cargo check --workspace`
- `test` — `cargo test --workspace`
- `clippy` — `cargo clippy --workspace -- -D warnings`
- `fmt` — `cargo fmt --all -- --check`

**Runner**: `ubuntu-latest` (GitHub-hosted)

### build-iso.yml — Debian-based ISO

**Triggers**: Push to `main`, manual dispatch.

**Produces**: `agnos-{version}-x86_64.iso` + SHA256

**Script**: `scripts/build-iso.sh`

**Notes**: This ISO uses Debian Trixie as a base with AGNOS userland on top.
It includes `/usr/src/agnos` with the full source tree for self-hosting.

### selfhost-build.yml — Self-Hosting Build

**Triggers**: Push changes to `scripts/`, `recipes/base/`, `kernel/config/`; manual dispatch.

**Runner**: `[self-hosted, linux, x64]` — requires root, 20 GB+ disk, 2+ hours.

**Jobs** (sequential):

1. **bootstrap-toolchain** — Downloads source tarballs, runs `bootstrap-toolchain.sh`
   to produce a cross-compiler (`x86_64-agnos-linux-gnu-gcc`). Uploads as artifact.

2. **build-base** — Downloads toolchain artifact, enters chroot, builds base
   packages with `ark-build.sh`. Builds AGNOS userland with `cargo`. Uploads rootfs.

3. **create-iso** — Downloads rootfs artifact, produces bootable ISO.

**Artifacts**:
- `agnos-toolchain-x86_64` (1.8 GB, 30-day retention)
- `agnos-rootfs-x86_64` (variable, 30-day retention)
- `agnos-iso-x86_64` (variable, 14-day retention)

### selfhost-validate.yml — QEMU Validation

**Triggers**: After `selfhost-build.yml` succeeds; manual dispatch.

**Runner**: `[self-hosted, linux, x64]` — requires KVM, QEMU.

**What it does**:
1. Downloads the ISO artifact
2. Boots it in QEMU with serial console
3. Waits for login prompt
4. Runs `selfhost-validate --quick --phase toolchain`
5. Verifies `/usr/src/agnos` source tree is present
6. Uploads boot log as artifact

### publish-toolchain.yml — Artifact Distribution

**Triggers**: After `selfhost-build.yml` succeeds; manual dispatch.

**Produces**:
- **GitHub Release**: `toolchain-{version}` and `toolchain-latest` tags
  with compressed toolchain (`agnos-toolchain-x86_64.tar.zst`)
- **Container Image**: `ghcr.io/maccracken/agnos-toolchain:{version}` and `:latest`

## Using Artifacts

### In another workflow (same repo)

```yaml
- uses: actions/download-artifact@v4
  with:
    name: agnos-toolchain-x86_64
    path: output/
    run-id: ${{ github.event.workflow_run.id }}
```

### In a consumer project (different repo)

**Via GitHub Release** (recommended):
```yaml
- name: Download AGNOS toolchain
  run: |
    curl -fSL -o toolchain.tar.zst \
      https://github.com/MacCracken/agnosticos/releases/download/toolchain-latest/agnos-toolchain-x86_64.tar.zst
    mkdir -p /tmp/agnos-sysroot
    zstd -d toolchain.tar.zst -o - | tar xf - -C /tmp/agnos-sysroot
    export PATH="/tmp/agnos-sysroot/tools/bin:$PATH"
    x86_64-agnos-linux-gnu-gcc --version
```

**Via Container Image**:
```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    container: ghcr.io/maccracken/agnos-toolchain:latest
    steps:
      - uses: actions/checkout@v4
      - run: x86_64-agnos-linux-gnu-gcc --version
      - run: cargo build --release
```

### Local development

```bash
# Restore cached toolchain (if previously built)
tar xf output/agnos-toolchain-x86_64.tar -C /tmp/agnos-selfhost

# Or download from release
curl -fSL -o toolchain.tar.zst \
  https://github.com/MacCracken/agnosticos/releases/download/toolchain-latest/agnos-toolchain-x86_64.tar.zst
mkdir -p /tmp/agnos-sysroot
zstd -d toolchain.tar.zst | tar xf - -C /tmp/agnos-sysroot

# Use the cross-compiler
export PATH="/tmp/agnos-sysroot/tools/bin:$PATH"
x86_64-agnos-linux-gnu-gcc hello.c -o hello
```

## Docker Images

### Runtime image (for consumer projects)

```
ghcr.io/maccracken/agnosticos:latest
ghcr.io/maccracken/agnosticos:pre-beta
ghcr.io/maccracken/agnosticos:{version}
```

Used as runtime base by consumer projects (BullShift, SecureYeoman, etc.).
Contains AGNOS userland binaries but NOT the build toolchain.

### Toolchain image (for building)

```
ghcr.io/maccracken/agnos-toolchain:latest
ghcr.io/maccracken/agnos-toolchain:{version}
```

Contains:
- `x86_64-agnos-linux-gnu-gcc` 15.2.0 (cross-compiler)
- Binutils 2.45
- Glibc 2.42 headers + libs
- Rust stable + cargo
- Build essentials (make, etc.)

### Building images locally

```bash
# Runtime image
docker build -t agnos:local -f docker/Dockerfile .

# Toolchain image (after bootstrap)
docker build -f /tmp/Dockerfile.toolchain -t agnos-toolchain:local .
```

## Self-Hosted Runner Setup

The self-hosting build and QEMU validation require a self-hosted runner.

### Requirements

- Linux x86_64 (your mini PC or Dell servers)
- Root access (for chroot/mount)
- 20 GB+ free disk space
- QEMU with KVM support
- zstd, squashfs-tools, grub, xorriso

### Installation

```bash
# Install dependencies (Arch Linux)
sudo pacman -S qemu-full squashfs-tools grub libisoburn mtools zstd

# Set up GitHub Actions runner
# Follow: https://docs.github.com/en/actions/hosting-your-own-runners
mkdir ~/actions-runner && cd ~/actions-runner
curl -o actions-runner.tar.gz -L https://github.com/actions/runner/releases/latest/download/actions-runner-linux-x64-2.321.0.tar.gz
tar xzf actions-runner.tar.gz
./config.sh --url https://github.com/MacCracken/agnosticos --token YOUR_TOKEN
./run.sh  # or install as systemd service
```

### Labels

Configure the runner with labels:
- `self-hosted`
- `linux`
- `x64`

Workflows use `runs-on: [self-hosted, linux, x64]` to target these runners.

## Consumer Project CI Pattern

All consumer projects (Nazar, Selah, Abaco, Rahd, etc.) follow the same CI/CD pattern:

```yaml
# .github/workflows/ci.yml
name: CI
on:
  push:
    branches: [main]
  pull_request:
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo check --workspace
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --workspace
  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy }
      - run: cargo clippy --workspace -- -D warnings
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt }
      - run: cargo fmt --all -- --check
```

```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    tags: ['*']
permissions:
  contents: write
jobs:
  build:
    strategy:
      matrix:
        include:
          - { target: x86_64-unknown-linux-gnu, os: ubuntu-latest }
          - { target: aarch64-unknown-linux-gnu, os: ubuntu-latest }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: '${{ matrix.target }}' }
      - run: cargo build --release --target ${{ matrix.target }}
      # ... package + upload
  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: softprops/action-gh-release@v2
        with:
          files: release/*
          generate_release_notes: true
```

### Release → AGNOS pickup flow

```
Tag version in consumer repo
  → release.yml builds amd64 + arm64 binaries
  → GitHub Release created with assets
  → ark-bundle.sh reads github_release from marketplace recipe
  → Fetches latest release asset
  → Produces .agnos-agent tarball
  → Installs via mela marketplace
```

## Troubleshooting

### Clippy failures

```bash
# Run locally first
cargo clippy --workspace -- -D warnings

# Common fix: doc comment formatting
# Add blank lines between doc sections
```

### Format failures

```bash
cargo fmt --all        # fix
cargo fmt --all -- --check  # verify
```

### Self-hosted runner disk full

```bash
# Clean old toolchain artifacts
rm -rf /tmp/agnos-selfhost
# Clean cargo cache
cargo clean
# Clean docker
docker system prune -af
```

### QEMU validation timeout

The ISO must boot within 10 minutes. If it times out:
1. Check kernel boot params in grub.cfg
2. Verify `live-boot` is installed in the rootfs
3. Test locally: `qemu-system-x86_64 -m 2G -cdrom output/agnos-*.iso -nographic`
