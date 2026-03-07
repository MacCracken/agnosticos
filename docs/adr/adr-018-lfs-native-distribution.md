# ADR-018: LFS-Native Distribution (Dropping Debian Dependency)

**Status:** Accepted
**Date:** 2026-03-07
**Supersedes:** Debian base image references in ADR-014

## Context

AGNOS currently assumes Debian Bookworm as its base distribution. This was a pragmatic early choice for ML ecosystem compatibility. However, AGNOS's long-term vision is to be a purpose-built AI-native OS, not a Debian derivative.

Every Debian package we carry is attack surface we don't control, update cycles we don't own, and bloat our users don't need. An AI agent OS needs ~50 carefully chosen packages, not the 30,000+ in Debian's repos.

Linux From Scratch (LFS) demonstrates that a fully functional Linux system can be built from source with complete control over every component. AGNOS should adopt this approach, with `ark` as the sole package manager.

## Decision

Build AGNOS as an independent Linux distribution using an LFS-inspired source build system. No dependency on Debian, Ubuntu, Arch, or any upstream distro. `ark` is the only package manager. The system is minimal by design — every installed package exists for a reason.

### Architecture

```
AGNOS Distribution Stack:

  +----------------------------------------------------------+
  |  AGNOS Applications                                       |
  |  agent-runtime, llm-gateway, ai-shell, desktop-env       |
  +----------------------------------------------------------+
  |  ark (sole package manager)                               |
  |  .ark packages = signed tarballs + metadata               |
  +----------------------------------------------------------+
  |  AGNOS Base System (~50 packages, built from source)      |
  |  glibc, coreutils, bash, openssl, python, rust toolchain  |
  +----------------------------------------------------------+
  |  AGNOS Init (custom, minimal)                             |
  |  Starts: agent-runtime, llm-gateway, compositor           |
  +----------------------------------------------------------+
  |  Linux Kernel 6.6 LTS (hardened, custom config)           |
  +----------------------------------------------------------+
  |  Hardware / Firmware                                       |
  +----------------------------------------------------------+
```

### Package Format: `.ark`

A new package format for system-level packages (distinct from `.agnos-agent` marketplace packages):

```
package-name-1.0.0.ark
  +-- MANIFEST        # Package metadata (JSON)
  +-- SIGNATURE       # Ed25519 signature (sigil)
  +-- DEPENDS         # Dependency list
  +-- FILES           # File manifest with SHA-256 hashes
  +-- data.tar.zst    # Actual file contents (zstd compressed)
```

**MANIFEST format:**
```json
{
  "name": "openssl",
  "version": "3.5.2",
  "release": 1,
  "description": "TLS/SSL cryptographic library",
  "arch": "x86_64",
  "size_installed": 12582912,
  "build_date": "2026-03-07T00:00:00Z",
  "builder": "takumi/0.1.0",
  "source_url": "https://www.openssl.org/source/openssl-3.5.2.tar.gz",
  "source_hash": "sha256:...",
  "license": "Apache-2.0",
  "groups": ["base", "crypto"]
}
```

### Build System: `takumi`

Build recipes for compiling packages from source. Inspired by Arch PKGBUILDs but in TOML.

```
build/
  recipes/
    base/
      glibc.toml
      coreutils.toml
      bash.toml
      openssl.toml
      python.toml
      ...
    toolchain/
      gcc.toml
      binutils.toml
      rust.toml
    ai/
      cuda-toolkit.toml
      onnxruntime.toml
      pytorch.toml
    desktop/
      wayland.toml
      mesa.toml
      libinput.toml
```

**Recipe format (`openssl.toml`):**
```toml
[package]
name = "openssl"
version = "3.5.2"
description = "TLS/SSL cryptographic library"
license = "Apache-2.0"
groups = ["base", "crypto"]

[source]
url = "https://www.openssl.org/source/openssl-3.5.2.tar.gz"
sha256 = "..."

[depends]
runtime = ["glibc", "zlib"]
build = ["perl", "make"]

[build]
configure = "./config --prefix=/usr --openssldir=/etc/ssl --libdir=lib shared zlib-dynamic"
make = "make -j$(nproc)"
check = "make test"
install = "make DESTDIR=$PKG install"

[security]
hardening = ["pie", "relro", "fortify"]
# Extra CFLAGS for security hardening
cflags = "-fstack-protector-strong -D_FORTIFY_SOURCE=2"
```

### Base System Package Set (~50 packages)

#### Tier 0: Toolchain (built first, cross-compiled)
| Package | Purpose |
|---------|---------|
| linux-api-headers | Kernel headers for glibc |
| glibc | C standard library |
| binutils | Assembler, linker |
| gcc | C/C++ compiler |

#### Tier 1: Core Utilities
| Package | Purpose |
|---------|---------|
| coreutils | ls, cp, mv, cat, etc. |
| bash | Shell |
| findutils | find, xargs |
| grep | Pattern matching |
| sed | Stream editor |
| gawk | Text processing |
| tar | Archive tool |
| gzip, bzip2, xz, zstd | Compression |
| make | Build tool |
| patch | Patching |
| file | File type detection |
| util-linux | mount, fdisk, lsblk, etc. |
| procps-ng | ps, top, free, etc. |
| kmod | Module loading |
| iproute2 | ip, ss, etc. |
| e2fsprogs | ext4 filesystem tools |

#### Tier 2: Security and Crypto
| Package | Purpose |
|---------|---------|
| openssl | TLS/SSL |
| libcap | POSIX capabilities |
| shadow | User management, passwd |
| acl | Access control lists |
| attr | Extended attributes |
| libxcrypt | Password hashing |
| linux-pam | Pluggable auth modules |

#### Tier 3: Language Runtimes
| Package | Purpose |
|---------|---------|
| python | Required for ML ecosystem |
| rust (pre-built) | AGNOS userland is Rust |
| perl | Build dependency for many packages |

#### Tier 4: Libraries
| Package | Purpose |
|---------|---------|
| zlib | Compression |
| readline | Line editing |
| ncurses | Terminal UI |
| expat | XML parsing |
| libffi | Foreign function interface |
| gdbm | Key-value database |
| openssl | Crypto (also in Tier 2) |

#### Tier 5: System Services
| Package | Purpose |
|---------|---------|
| eudev | Device management (no systemd) |
| dbus | IPC (needed by Wayland) |
| nftables | Firewall |

#### Tier 6: AI/ML (optional, ark-installable)
| Package | Purpose |
|---------|---------|
| cuda-toolkit | NVIDIA GPU compute |
| onnxruntime | Model inference |
| pytorch | ML framework |
| numpy | Numerical computing |

#### Tier 7: Desktop (optional, ark-installable)
| Package | Purpose |
|---------|---------|
| wayland | Display protocol |
| mesa | GPU drivers |
| libinput | Input handling |
| fontconfig + fonts | Text rendering |

### Init System: `argonaut`

Custom minimal init — the heroes who launch the system. NOT systemd, NOT sysvinit. Purpose-built for AGNOS. (Greek: Argonauts sailed the Argo — one letter off from AGNOS.)

```
Boot sequence:
  1. Kernel boots, runs /sbin/init (argonaut)
  2. Mount filesystems (/proc, /sys, /dev, /run)
  3. Start eudev (device manager)
  4. Mount root filesystem (dm-verity verified)
  5. Start aegis (security daemon) -- integrity checks
  6. Start agent-runtime (port 8090)
  7. Start llm-gateway (port 8088)
  8. Start compositor (if desktop mode)
  9. Start agnsh (login shell)
```

argonaut is a single Rust binary. No shell scripts in the boot path. No runlevels. Three modes:
- **server**: agent-runtime + llm-gateway only (headless)
- **desktop**: + compositor + shell
- **minimal**: agent-runtime only (container/embedded)

### Package Repository

Binary packages hosted at `packages.agnos.org`:

```
packages.agnos.org/
  v2026/
    x86_64/
      base/
        glibc-2.42-1.ark
        coreutils-9.7-1.ark
        ...
      ai/
        cuda-toolkit-12.8-1.ark
        pytorch-2.6-1.ark
        ...
      desktop/
        wayland-1.23-1.ark
        mesa-25.1-1.ark
        ...
    aarch64/
      base/
        ...
    REPO.json         # Package index (signed)
    REPO.sig          # sigil signature
```

ark downloads pre-built `.ark` packages by default. Source builds are available via `ark install --source <package>` for users who want custom flags.

### Migration Path

This is a phased transition, not a big bang:

**Phase 1 (current):** Debian base, ark wraps apt. Ship alpha.
**Phase 2:** Build LFS-style base system. ark can install .ark packages alongside apt.
**Phase 3:** Default images use AGNOS base. Debian compatibility layer available.
**Phase 4:** Drop Debian entirely. ark + .ark packages only.

The alpha ships with Debian. Post-alpha, we transition. The userland Rust code doesn't change at all — it's already distro-agnostic.

## Consequences

### Positive
- Complete control over every binary on the system
- Minimal attack surface (~50 packages vs ~500+ in Debian minimal)
- Every package built with AGNOS security flags (PIE, RELRO, stack protector, FORTIFY)
- No upstream distro dependency, release cycle, or breakage
- Smaller images (~200-300MB vs 800MB+)
- ark is the single source of truth for all packages
- argonaut init boots in <3 seconds (no systemd overhead)
- sigil verifies every package from source hash to installed binary

### Negative
- Significant build infrastructure investment (CI farm, package repo)
- Must maintain ~50 package recipes and keep them updated
- Security patches are our responsibility (no Debian security team)
- ML ecosystem packages (CUDA, PyTorch) are complex to build
- Longer time to first stable release
- Smaller community for troubleshooting base system issues

### Mitigations
- CUDA toolkit: use NVIDIA's pre-built binaries (they provide tarballs, not just .deb)
- PyTorch: build from source with our toolchain (they support this)
- Security patches: automated CVE monitoring + rebuild pipeline
- Build infra: GitHub Actions for CI, self-hosted runners for large builds
- Pre-built Rust toolchain: download official rustup binaries (cross-platform)

## Alternatives Considered

### Stay on Debian
Rejected. Long-term dependency on upstream we don't control. Bloated base. Not a real OS, just a layer.

### Use Alpine/musl
Rejected. musl breaks too much of the ML ecosystem (numpy, torch, etc. expect glibc).

### Use NixOS
Rejected. Nix store model conflicts with ark's simpler /usr layout. CUDA support is painful. Learning curve is extreme.

### Use Arch Linux
Rejected. Rolling release is unstable for production OS. AUR is untrusted. But Arch's PKGBUILD format inspired our recipe format.

## Related

- ADR-014: Cross-Project Integration (superseded Debian base image references)
- ADR-015: Agent Marketplace Architecture (.agnos-agent format for agents, .ark for system)
- [Linux From Scratch](https://www.linuxfromscratch.org/) — reference implementation
