# AGNOS Development Roadmap

> **Status**: Pre-Beta | **Last Updated**: 2026-03-10
> **Userland complete** — 9878+ tests (3027 agent-runtime), ~82% coverage, 0 warnings
> **Base system**: 108 base + 84 Phase 11 recipes | **Audit**: 15 rounds complete
> **Next Milestone**: Beta Release (Target: Q4 2026)

---

## Beta Goal

AGNOS boots as an **independent Linux distribution** — no Debian, no Ubuntu, no
host distro. A self-hosting LFS-style base system built entirely from source via
takumi recipes, with ark as the sole package manager. The userland (daimon,
hoosh, agnoshi, aethersafha, etc.) runs on top of a base system we control from
toolchain to init.

Reference: [Linux From Scratch 12.4](https://www.linuxfromscratch.org/lfs/view/stable/)
(77 packages) + [Beyond LFS](https://www.linuxfromscratch.org/blfs/view/stable/)
for desktop/networking/GPU stack.

---

## Phase 10 — LFS Base System

Everything in this phase produces takumi recipes under `recipes/base/`.
Build order follows LFS chapter structure. All packages get security
hardening flags (PIE, full RELRO, FORTIFY_SOURCE=2, stack protector, BINDNOW).

### 10A — Cross-Toolchain (LFS Ch. 5–6)

Bootstrap a cross-compiler that doesn't depend on the host.

| # | Package | Version | Recipe | Notes |
|---|---------|---------|--------|-------|
| 1 | linux-api-headers | 6.6.x | `linux-api-headers.toml` | Kernel headers only (no build) |
| 2 | glibc | 2.42 | `glibc.toml` | C library — Pass 1 cross, Pass 2 final |
| 3 | binutils | 2.45 | `binutils.toml` | Linker/assembler — Pass 1 + Pass 2 |
| 4 | gcc | 15.2.0 | `gcc.toml` | Compiler — Pass 1 (cross) + Pass 2 (native) |
| 5 | gmp | 6.3.0 | `gmp.toml` | GCC dependency |
| 6 | mpfr | 4.2.2 | `mpfr.toml` | GCC dependency |
| 7 | mpc | 1.3.1 | `mpc.toml` | GCC dependency |
| 8 | libstdc++ | (GCC) | — | Built as part of GCC |

**Deliverable**: Self-hosting toolchain that can compile everything else.

### 10B — Core Utilities (LFS Ch. 6–8)

Temporary tools first, then final system versions.

| # | Package | Version | Group | Notes |
|---|---------|---------|-------|-------|
| 1 | coreutils | 9.7 | core | ls, cp, mv, cat, chmod, etc. |
| 2 | bash | 5.3 | core | Default shell |
| 3 | findutils | 4.10.0 | core | find, xargs |
| 4 | grep | 3.12 | core | Pattern matching |
| 5 | sed | 4.9 | core | Stream editor |
| 6 | gawk | 5.3.2 | core | Text processing |
| 7 | tar | 1.35 | core | Archive tool |
| 8 | gzip | 1.14 | core | Compression |
| 9 | xz | 5.8.1 | core | LZMA compression |
| 10 | bzip2 | 1.0.8 | core | Compression |
| 11 | zstd | 1.5.7 | core | Fast compression |
| 12 | lz4 | 1.10.0 | core | Fastest compression |
| 13 | diffutils | 3.12 | core | diff, cmp |
| 14 | patch | 2.8 | core | Apply diffs |
| 15 | make | 4.4.1 | core | Build tool |
| 16 | file | 5.46 | core | File type detection |
| 17 | m4 | 1.4.20 | core | Macro processor |
| 18 | bc | 7.0.3 | core | Calculator |
| 19 | less | 679 | core | Pager |
| 20 | which | 2.21 | core | Command lookup |

**Deliverable**: 20 recipes, basic userland functional.

### 10C — System Libraries (LFS Ch. 8)

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | zlib | 1.3.1 | Universal compression lib |
| 2 | readline | 8.3 | Line editing |
| 3 | ncurses | 6.5 | Terminal UI |
| 4 | libffi | 3.5.2 | Foreign function interface |
| 5 | expat | 2.7.1 | XML parser |
| 6 | gdbm | 1.26 | Database library |
| 7 | attr | 2.5.2 | Extended attributes |
| 8 | acl | 2.3.2 | Access control lists |
| 9 | libcap | 2.76 | POSIX capabilities |
| 10 | libxcrypt | 4.4.38 | Password hashing |
| 11 | libtool | 2.5.4 | Shared library support |
| 12 | gperf | 3.3 | Perfect hash generator |
| 13 | libpipeline | 1.5.8 | Pipeline manipulation |
| 14 | libelf (elfutils) | 0.193 | ELF handling |

**Deliverable**: 14 recipes, all shared libraries AGNOS needs.

### 10D — Security & Crypto

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | openssl | 3.5.2 | TLS/crypto (already dep in recipes) |
| 2 | shadow | 4.18.0 | User/group management, passwd, login |
| 3 | linux-pam | 1.7.1 | Pluggable auth (integrates with `pam.rs`) |
| 4 | sudo | 1.9.17p2 | Privilege escalation (complements shakti) |
| 5 | gnupg | 2.4.8 | Package signing verification |
| 6 | gnutls | 3.8.10 | TLS (alternative to OpenSSL for some deps) |
| 7 | p11-kit | 0.25.5 | PKCS#11 module management |
| 8 | openssh | 10.0p1 | Remote access |
| 9 | cryptsetup | 2.8.1 | LUKS encryption (integrates with `luks.rs`) |
| 10 | nftables | 1.1.1 | Firewall (replaces iptables) |
| 11 | libseccomp | 2.5.5 | Seccomp helper (used by sandbox) |
| 12 | audit | 4.0.2 | Linux audit framework |

**Deliverable**: 12 recipes, full security stack standalone.

### 10E — System Services & Init

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | util-linux | 2.41.1 | mount, fdisk, lsblk, etc. (~50 tools) |
| 2 | procps-ng | 4.0.5 | ps, top, free, etc. |
| 3 | psmisc | 23.7 | killall, fuser, pstree |
| 4 | iproute2 | 6.16.0 | ip, ss, tc (networking) |
| 5 | kbd | 2.8.0 | Keyboard tools |
| 6 | kmod | 34.2 | Module loading (modprobe, lsmod) |
| 7 | eudev | 3.2.14 | Device manager (replaces systemd-udevd) |
| 8 | dbus | 1.16.2 | Message bus (required by desktop) |
| 9 | e2fsprogs | 1.47.3 | ext4 filesystem tools |
| 10 | dosfstools | 4.2 | FAT/EFI filesystem tools |
| 11 | inetutils | 2.6 | hostname, ping, telnet, etc. |
| 12 | sysklogd | 2.7.2 | System logging |
| 13 | sysvinit | 3.14 | Init (placeholder — argonaut replaces) |
| 14 | iana-etc | 20250807 | /etc/services, /etc/protocols |

**Deliverable**: 14 recipes, system can boot and manage hardware.

### 10F — Build Tools & Languages

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | autoconf | 2.72 | Build system |
| 2 | automake | 1.18.1 | Build system |
| 3 | pkgconf | 2.5.1 | Package config |
| 4 | cmake | 4.1.0 | Build system (BLFS dep) |
| 5 | ninja | 1.13.1 | Fast build system |
| 6 | meson | 1.8.3 | Build system (Wayland/Mesa need it) |
| 7 | perl | 5.42.0 | Required by autotools, kernel build |
| 8 | python | 3.13.7 | Already have recipe, promote to base |
| 9 | rust | 1.89.0 | Needed to compile AGNOS userland |
| 10 | flex | 2.6.4 | Lexer generator |
| 11 | bison | 3.8.2 | Parser generator |
| 12 | gettext | 0.26 | i18n |
| 13 | texinfo | 7.2 | Documentation |
| 14 | groff | 1.23.0 | Man page formatting |
| 15 | man-db | 2.13.1 | Man page viewer |
| 16 | man-pages | 6.15 | Linux man pages |

**Deliverable**: 16 recipes, system is self-hosting (can rebuild itself).

### 10G — Kernel & Bootloader

| # | Item | Notes |
|---|------|-------|
| 1 | Linux kernel 6.6 LTS | Config exists (`agnos_defconfig`), needs recipe + CI |
| 2 | AGNOS kernel modules | 4 C modules (agent-subsystem, security, audit, llm) |
| 3 | GRUB 2.12 | Bootloader recipe + EFI support |
| 4 | dracut or mkinitcpio | Initramfs generator |
| 5 | firmware blobs | linux-firmware package for WiFi/GPU |
| 6 | Kernel signing | Secure Boot with AGNOS signing key |

**Deliverable**: Bootable system from bare metal.

**Phase 10 Total: ~108 base system recipes**

---

## Phase 11 — Desktop & Networking Stack (BLFS)

Packages needed to run aethersafha (Wayland compositor) and connect to
the network. Recipes go in `recipes/desktop/` and `recipes/network/`.

### 11A — Graphics Stack

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | wayland | 1.24.0 | Core Wayland protocol |
| 2 | wayland-protocols | 1.45 | Extended protocols (xdg-shell, etc.) |
| 3 | mesa | 25.1.x | OpenGL/Vulkan/EGL (GPU drivers) |
| 4 | libdrm | 2.4.125 | Direct rendering manager |
| 5 | libinput | 1.27.x | Input device handling |
| 6 | libxkbcommon | 1.11.0 | Keyboard handling |
| 7 | vulkan-headers | 1.4.x | Vulkan API headers |
| 8 | vulkan-loader | 1.4.x | Vulkan ICD loader |
| 9 | libepoxy | 1.5.10 | GL dispatch library |
| 10 | pixman | 0.44.x | Pixel manipulation |
| 11 | cairo | 1.18.x | 2D graphics |
| 12 | pango | 1.56.x | Text rendering |
| 13 | harfbuzz | 10.x | Text shaping |
| 14 | freetype | 2.13.x | Font rendering |
| 15 | fontconfig | 2.16.x | Font configuration |
| 16 | xwayland | 24.1.x | X11 compatibility |
| 17 | wlroots | 0.18.x | Compositor library (if aethersafha uses it) |

### 11B — Audio Stack

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | alsa-lib | 1.2.14 | ALSA userspace |
| 2 | alsa-utils | 1.2.14 | amixer, aplay, etc. |
| 3 | pipewire | 1.4.x | Audio/video routing (modern replacement for PulseAudio) |
| 4 | wireplumber | 0.5.x | PipeWire session manager |
| 5 | libsndfile | 1.2.x | Audio file I/O |

### 11C — Networking

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | curl | 8.15.x | HTTP client |
| 2 | wget | 1.25.x | Download tool |
| 3 | openssh | 10.0p1 | (also in 10D, shared) |
| 4 | dhcpcd | 10.2.x | DHCP client |
| 5 | wpa_supplicant | 2.11 | WiFi auth |
| 6 | iw | 6.9 | WiFi config |
| 7 | networkmanager | 1.54.x | Network management daemon |
| 8 | rsync | 3.4.x | File sync |
| 9 | ca-certificates | 2025.x | TLS root CAs |
| 10 | dns-utils (bind-utils) | 9.20.x | dig, nslookup |

### 11D — Desktop Support Libraries

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | glib | 2.84.x | GObject, GIO, etc. |
| 2 | gobject-introspection | 1.84.x | Language bindings |
| 3 | gtk4 | 4.18.x | GTK toolkit (for Flutter Linux host) |
| 4 | libnotify | 0.8.x | Desktop notifications |
| 5 | json-glib | 1.10.x | JSON for GLib |
| 6 | polkit | 125 | Authorization framework |
| 7 | elogind | 255.x | Session management (no systemd) |
| 8 | udisks | 2.10.x | Disk management |
| 9 | upower | 1.90.x | Power management |
| 10 | gstreamer | 1.26.x | Multimedia framework |

### 11E — AI/ML Infrastructure

Packages needed to run ML/AI workloads natively. Recipes go in `recipes/ai/`.
Hoosh already provides 15 LLM providers; these packages make GPU compute,
model training, and inference available as first-class system capabilities.

| # | Package | Version | Notes |
|---|---------|---------|-------|
| 1 | nvidia-cuda-toolkit | 12.x | CUDA compiler, runtime, math libs (.ark) |
| 2 | rocm | 6.x | AMD GPU compute stack (HIP, rocBLAS, MIOpen) |
| 3 | openblas | 0.3.x | Optimized BLAS for CPU linear algebra |
| 4 | lapack | 3.12.x | Linear algebra routines (Fortran + C interface) |
| 5 | llama-cpp | latest | llama.cpp as system package (CUDA/ROCm/Vulkan backends) |
| 6 | ollama | latest | Local LLM runner (wraps llama.cpp, manages models) |
| 7 | onnxruntime | 1.x | ONNX model inference (CPU, CUDA, ROCm providers) |
| 8 | vllm | latest | High-throughput LLM serving (PagedAttention) |
| 9 | python-numpy | 1.26.x | NumPy as .ark (links OpenBLAS) |
| 10 | python-scipy | 1.14.x | Scientific computing |
| 11 | python-pandas | 2.2.x | Data manipulation |
| 12 | python-pytorch | 2.x | PyTorch (CPU + CUDA + ROCm variants) |
| 13 | python-transformers | 4.x | HuggingFace transformers library |
| 14 | python-safetensors | 0.4.x | Safe model serialization format |
| 15 | nccl | 2.x | NVIDIA multi-GPU communication |
| 16 | podman | 5.x | Rootless container runtime (OCI-compatible) |
| 17 | crun | 1.x | OCI runtime (lightweight, rootless) |
| 18 | jupyter-server | 2.x | Jupyter notebook/lab server |
| 19 | vulkan-compute-tools | — | Vulkan SPIR-V compiler + validation layers for GPU compute |
| 20 | huggingface-hub-cli | latest | Model download/management CLI |

**Phase 11 Total: ~62 desktop/networking/AI recipes**

---

## Phase 12 — System Integration

Wire the LFS base system into AGNOS's own tooling.

### 12A — Argonaut as Real Init

| # | Item | Notes |
|---|------|-------|
| 1 | Replace sysvinit with argonaut | Boot stages → real service management |
| 2 | Service dependency graph | PostgreSQL before daimon, dbus before aethersafha |
| 3 | Runlevel equivalents | Console, Server, Desktop boot modes |
| 4 | Shutdown/reboot orchestration | Signal agents, flush state, unmount |
| 5 | Emergency/rescue shell | Drop to agnoshi on boot failure |

### 12B — Ark as Sole Package Manager

| # | Item | Notes |
|---|------|-------|
| 1 | Dependency resolution via nous | resolve install order from recipe `[depends]` |
| 2 | `ark install <package>` from registry | Download .ark, verify sigil signature, install |
| 3 | `ark remove <package>` | Track installed files, clean removal |
| 4 | `ark upgrade` | Check registry for newer versions, upgrade in-place |
| 5 | `ark search` | Query local and remote package index |
| 6 | Transaction log | Atomic installs, rollback on failure |
| 7 | `/var/lib/ark/installed.db` | Package database (installed files, versions, checksums) |

### 12C — Agnova Installer (Real)

| # | Item | Notes |
|---|------|-------|
| 1 | Disk partitioning (GPT + EFI) | Use `fdisk`/`parted` from util-linux |
| 2 | Filesystem creation (ext4, btrfs) | mkfs from e2fsprogs |
| 3 | Base system extraction | Unpack base .ark packages to target |
| 4 | GRUB installation | EFI + BIOS boot |
| 5 | User creation | First-boot user setup via shadow |
| 6 | Network configuration | dhcpcd/NetworkManager setup |
| 7 | Locale/timezone setup | From base system |
| 8 | ISO generation | Bootable install media with agnova TUI |

### 12D — Build Reproducibility & CI

| # | Item | Notes |
|---|------|-------|
| 1 | Takumi builder container | Build all ~130 recipes in Docker |
| 2 | Deterministic builds | SOURCE_DATE_EPOCH, stripped paths |
| 3 | Package registry | Host .ark files (S3/GHCR/self-hosted) |
| 4 | CI pipeline | Build base system nightly, test boot in QEMU |
| 5 | SBOM generation | Per-package SBOM, aggregate system SBOM |
| 6 | QEMU boot test | CI job that boots the ISO, runs health checks |

---

## Phase 13 — Beta Polish

### 13A — Self-Hosting Validation

| # | Item | Notes |
|---|------|-------|
| 1 | Build AGNOS on AGNOS | Full bootstrap: compile GCC, Rust, kernel on the built system |
| 2 | Kernel module build on target | Compile AGNOS kernel modules without host |
| 3 | Userland rebuild on target | `cargo build` of agent-runtime, llm-gateway, etc. |
| 4 | Package rebuild on target | `ark-build.sh` works inside AGNOS |

### 13B — Hardware Support

| # | Item | Notes |
|---|------|-------|
| 1 | NVIDIA GPU (proprietary driver) | .ark recipe for nvidia-driver |
| 2 | NVIDIA GPU (nouveau/open) | Mesa nouveau driver |
| 3 | AMD GPU (amdgpu) | Mesa radeonsi driver |
| 4 | Intel GPU (i915) | Mesa iris driver |
| 5 | WiFi firmware | linux-firmware .ark package |
| 6 | Bluetooth | bluez .ark recipe |
| 7 | USB/Thunderbolt | bolt daemon |
| 8 | Printer support | CUPS (optional) |

### 13C — Community & Documentation

| # | Item | Notes |
|---|------|-------|
| 1 | Installation guide | Step-by-step for bare metal + VM |
| 2 | Video tutorials | Installation, usage, agent creation |
| 3 | Kernel development guide | Contributing to AGNOS kernel modules |
| 4 | Support portal | Discord + forum |
| 5 | Community testing program | Beta tester enrollment |
| 6 | Third-party security audit | External vendor (previously P1 alpha blocker) |
| 7 | Bug tracker triage | Public issue templates |

### 13D — Consumer App Integration

| # | App | Recipe | Status | Notes |
|---|-----|--------|--------|-------|
| 1 | SecureYeoman | `recipes/marketplace/secureyeoman.toml` | Ready | Flagship, 20,500+ tests, 279 MCP tools |
| 2 | Photis Nadi | `recipes/marketplace/photisnadi.toml` | Ready | Flutter productivity, 389 tests |
| 3 | BullShift | `recipes/marketplace/bullshift.toml` | Ready | Trading platform, 552 tests |
| 4 | AGNOSTIC | `recipes/marketplace/agnostic.toml` | Stub | Python/CrewAI QA platform |
| 5 | **Delta** | `recipes/marketplace/delta.toml` | **Recipe created** | Code hosting (port 8070), CI/CD, artifact registry. 49 tests. Needs: mela listing, agnoshi `delta` intent, daimon health consumer |

#### Delta AGNOS-Side Integration Items

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Marketplace recipe | Done | `recipes/marketplace/delta.toml` — Rust binary build, systemd service |
| 2 | Mela listing | Not started | Register Delta in marketplace registry with metadata |
| 3 | Agnoshi `delta` intent | Not started | Shell commands: `delta create-repo`, `delta pr`, `delta push` |
| 4 | Daimon consumer health | Not started | Add Delta to `/v1/health/consumers` monitoring |
| 5 | Documentation | Not started | Delta installation guide in AGNOS docs |

### 13E — Previous Alpha Items (Moved)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | Run browser-ark CI | Ready | `.github/workflows/browser-ark.yml` exists |
| 2 | Run marketplace-publish CI | Ready | `.github/workflows/marketplace-publish.yml` exists |
| 3 | Python runtime management (4 phases) | Not started | Shim, .python-version, venv, pip proxy |
| 4 | AI-integrated WebView | Not started | wry/tauri, hoosh integration |
| 5 | Docker base images on AGNOS base | Not started | Replace Debian base with AGNOS .ark base |

---

## Release Roadmap (Revised)

### Beta Release — Q4 2026

**Criteria:**
- [ ] Phase 10 complete — ~108 base system recipes, self-hosting toolchain
- [ ] Phase 11 complete — Desktop, networking & AI/ML stack (~62 recipes)
- [ ] Phase 12 complete — Argonaut init, ark package manager, agnova installer
- [ ] AGNOS boots from ISO on bare metal (UEFI) and QEMU
- [ ] Self-hosting: can rebuild itself from source
- [ ] Third-party security audit complete
- [ ] Community testing program active

### v1.0 Release — Q2 2027

**Criteria:**
- [ ] Phase 13 complete — Hardware support, documentation, community
- [ ] All consumer apps published to mela
- [ ] Python runtime management
- [ ] Enterprise features: SSO (done), audit logging (done), mTLS (done)
- [ ] 6 months of beta testing with no critical bugs
- [ ] Commercial support available

---

## Phase Summary

| Phase | Status | Tests | Key Deliverables |
|-------|--------|-------|------------------|
| 0-4 | Complete | — | Foundation through Desktop |
| 5 | Complete | — | Production hardening, module refactoring |
| 5.6 | Complete | — | All stubs eliminated |
| 6 | Complete | 200+ | Hardware acceleration, swarm, networking tools |
| 6.5 | Complete | 550+ | 16 OS-level modules |
| 6.6 | Complete | — | Consumer integration |
| 6.7 | Complete | 100+ | Alpha polish |
| 6.8 | Complete | 600+ | Beta features (RAG, RPC, OpenTelemetry, etc.) |
| 7 | Complete | 199 | Federation, migration, scheduling, ratings |
| 8A-8M | Complete | 703 | Distribution, PQC, AI safety, formal verification, RL |
| 9 | Complete | 169 | Cloud services, human-AI collaboration |
| 9.5 | Complete | 102 | Full convergence (OIDC, delegation, vector REST, marketplace) |
| **10** | **Complete** | — | **LFS base system (108 recipes)** |
| **11** | **Not started** | — | **Desktop, networking & AI/ML stack (~62 recipes)** |
| **12** | **Complete** | 148 | **System integration: argonaut (117), ark (49), agnova (91), CI** |
| **13** | **Not started** | — | **Beta polish, hardware, community** |

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-10)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~82% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 9600+ | Met |
| Agent Spawn Time | <500ms | ~300ms | Met |
| Shell Response Time | <100ms | ~50ms | Met |
| Memory Overhead | <2GB | ~1.2GB | Met |
| Boot Time | <10s | N/A | Pending (Phase 12) |
| CIS Compliance | >80% | ~85% | Met |
| Stub Implementations | 0 | 0 | Met |
| Compiler Warnings | 0 | 0 | Met |
| Base System Recipes | ~108 | 108 | Complete |
| Desktop/AI Stack Recipes | ~62 | 84 | Phase 11 (complete) |
| Self-Hosting | Yes | No | Phase 13 |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 3023+ | Orchestrator, IPC, sandbox, registry, marketplace (88+43), federation (73), migration (54), scheduler (51), PQC (68), explainability (59), safety (77), finetune (73), formal_verify (76), sandbox_v2 (79), rl_optimizer (68), cloud (82), collaboration (87), sigil (46), aegis (55), takumi (57), argonaut (117), agnova (91), ark (49), database (42), grpc (14), service_mesh (20), oidc (22), delegation (28), vector_rest (24), marketplace_backend (28) |
| llm-gateway | 710 | 15 providers (5 native + 10 OpenAI-compatible), rate limiting, streaming, cert pinning, hardware acceleration, token budgets |
| ai-shell | 1132 | 25+ intents, approval workflow, dashboard, aliases, completion |
| desktop-environment | 1447+ | Wayland protocol (63+49), screen capture (31), screen recording (22+), plugin host (31), xwayland (20), shell integration (26), theme bridge (18), compositor, renderer |

---

## Architecture Decision Records

| # | ADR | Status |
|---|-----|--------|
| 001 | Foundation and Architecture | Accepted |
| 002 | Agent Runtime and Lifecycle | Accepted |
| 003 | Security and Trust | Accepted |
| 004 | Distribution, Build, and Installation | Accepted |
| 005 | Desktop Environment | Accepted |
| 006 | Observability and Operations | Accepted |
| 007 | Scale, Collaboration, and Future | Accepted |

---

## Named Subsystems

| Name | Role | Component |
|------|------|-----------|
| **hoosh** | LLM inference gateway (port 8088, 15 providers) | `llm-gateway/` |
| **daimon** | Agent orchestrator (port 8090) | `agent-runtime/` |
| **agnosys** | Kernel interface | `agnos-sys/` |
| **agnostik** | Shared types library | `agnos-common/` |
| **shakti** | Privilege escalation | `agnos-sudo/` |
| **agnoshi** | AI shell (`agnsh`) | `ai-shell/` |
| **aethersafha** | Desktop compositor | `desktop-environment/` |
| **ark** | Unified package manager | `ark.rs`, `/v1/ark/*` |
| **nous** | Package resolver daemon | `nous.rs` |
| **takumi** | Package build system | `takumi.rs` |
| **mela** | Agent marketplace | `marketplace/` module |
| **aegis** | System security daemon | `aegis.rs` |
| **sigil** | Trust verification | `sigil.rs` |
| **argonaut** | Init system | `argonaut.rs` |
| **agnova** | OS installer | `agnova.rs` |
| **vansh** | Voice AI shell (planned) | TBD |

---

## Contributing

### Priority Contribution Areas

1. **Desktop & AI/ML recipes (Phase 11)** — Wayland, Mesa, PipeWire, CUDA, llama.cpp, etc.
2. **QEMU boot testing** — CI pipeline for automated boot validation
3. **Hardware testing** — GPU drivers, WiFi, Bluetooth on real hardware
4. **Documentation** — Installation guide, kernel dev guide
5. **SHA256 verification** — Fill in real checksums for all 108 base recipes

### Getting Started

See [CONTRIBUTING.md](/CONTRIBUTING.md) for:
- Development environment setup
- Code style and testing requirements
- Git workflow and commit conventions
- Pull request process

---

## Resources

- **Repository**: https://github.com/agnostos/agnos
- **Documentation**: https://docs.agnos.org (planned)
- **Issue Tracker**: https://github.com/agnostos/agnos/issues
- **Changelog**: [CHANGELOG.md](/CHANGELOG.md)
- **LFS Reference**: https://www.linuxfromscratch.org/lfs/view/stable/
- **BLFS Reference**: https://www.linuxfromscratch.org/blfs/view/stable/

---

*Last Updated: 2026-03-10 | Next Review: 2026-03-17*
