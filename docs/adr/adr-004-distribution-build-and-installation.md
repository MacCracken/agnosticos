# ADR-004: Distribution, Build, and Installation

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS is an independent Linux distribution. Rather than layering on Debian or Arch, AGNOS builds from source (LFS-inspired) with complete control over every binary. This enables a minimal attack surface, security hardening on every package, and a purpose-built boot sequence.

## Decisions

### LFS-Native Distribution

No dependency on Debian, Ubuntu, Arch, or any upstream distro. `ark` is the sole package manager. The system is minimal by design — ~50 carefully chosen packages, not 30,000+.

**Migration path** (phased):
1. Debian base for alpha (pragmatic)
2. LFS-style base system built, ark installs .ark alongside apt
3. Default images use AGNOS base
4. Drop Debian entirely

### Package Format: `.ark`

System-level packages (distinct from `.agnos-agent` marketplace packages):

```
package-name-1.0.0.ark
  +-- MANIFEST        # JSON metadata
  +-- SIGNATURE       # Ed25519 signature (sigil)
  +-- DEPENDS         # Dependency list
  +-- FILES           # File manifest with SHA-256 hashes
  +-- data.tar.zst    # File contents (zstd compressed)
```

Binary packages hosted at `packages.agnos.org`, organized by release/arch/group. ark downloads pre-built packages by default; `ark install --source` builds from recipe.

### Takumi Build System

TOML-based recipes for compiling packages from source, inspired by Arch PKGBUILDs:

```toml
[package]
name = "openssl"
version = "3.5.2"
license = "Apache-2.0"

[source]
url = "https://www.openssl.org/source/openssl-3.5.2.tar.gz"
sha256 = "..."

[depends]
runtime = ["glibc", "zlib"]
build = ["perl", "make"]

[build]
configure = "./config --prefix=/usr --openssldir=/etc/ssl shared zlib-dynamic"
make = "make -j$(nproc)"
install = "make DESTDIR=$PKG install"

[security]
hardening = ["pie", "relro", "fortify"]
```

All packages built with security flags: PIE, RELRO, stack protector, FORTIFY_SOURCE=2.

### Base System (~50 packages)

| Tier | Packages | Purpose |
|------|----------|---------|
| 0 | linux-api-headers, glibc, binutils, gcc | Toolchain |
| 1 | coreutils, bash, findutils, grep, sed, tar, util-linux, iproute2 | Core utilities |
| 2 | openssl, libcap, shadow, linux-pam | Security and crypto |
| 3 | python, rust, perl | Language runtimes |
| 4 | zlib, readline, ncurses, libffi | Libraries |
| 5 | eudev, dbus, nftables | System services |
| 6 | cuda-toolkit, onnxruntime, pytorch (optional) | AI/ML |
| 7 | wayland, mesa, libinput (optional) | Desktop |

### Argonaut Init System

Custom minimal init — a single Rust binary, no shell scripts in the boot path. Target: <3 seconds to agent-ready.

**Boot sequence:**
1. Kernel boots, runs `/sbin/init` (argonaut)
2. Mount filesystems (`/proc`, `/sys`, `/dev`, `/run`)
3. Start eudev (device manager)
4. Mount root filesystem (dm-verity verified)
5. Start aegis (security daemon — integrity checks)
6. Start daimon (agent-runtime, port 8090)
7. Start hoosh (llm-gateway, port 8088)
8. Start aethersafha (compositor, if desktop mode)
9. Start agnoshi (login shell)

**Three modes:**
- **Server** — daimon + hoosh only (headless)
- **Desktop** — + compositor + shell
- **Minimal** — daimon only (container/embedded)

### Agnova Installer

Purpose-built OS installer with security-by-default:

- **LUKS encryption** enabled by default
- **dm-verity** for root filesystem integrity
- **Secure Boot** support with signed bootloader
- **Four install modes**: Interactive, Automated (TOML config), Minimal, Recovery
- **Base footprint**: ~200-300MB (vs 800MB+ for Debian minimal)

## Consequences

### Positive
- Complete control over every binary on the system
- Minimal attack surface (~50 packages vs ~500+)
- Every package built with security hardening flags
- Argonaut boots in <3 seconds (no systemd overhead)
- Sigil verifies every package from source hash to installed binary
- Smaller images (~200-300MB)

### Negative
- Significant build infrastructure investment
- Must maintain ~50 package recipes and keep them updated
- Security patches are our responsibility
- ML ecosystem packages (CUDA, PyTorch) are complex to build
- Longer time to first stable release

### Mitigations
- CUDA: use NVIDIA's pre-built tarballs
- PyTorch: build from source (officially supported)
- Security patches: automated CVE monitoring + rebuild pipeline
- Pre-built Rust toolchain via official rustup binaries
