# AGNOS — A General Networked Operating System

> **A** **G**eneral **N**etworked **O**perating **S**ystem

[![License](https://img.shields.io/badge/license-GPLv3-blue)](LICENSE)
[![Kernel](https://img.shields.io/badge/kernel-AGNOS%201.57.10-orange)](https://github.com/MacCracken/agnos)
[![Language](https://img.shields.io/badge/Cyrius-6.6.8-red)](https://github.com/MacCracken/cyrius)
![Status](https://img.shields.io/badge/status-pre--beta-yellow)

**AGNOS** is a sovereign operating system written in **Cyrius** — a systems language with a 29KB seed, zero external dependencies, and a self-hosting compiler. The kernel boots to a typeable shell on real AMD hardware (Boot-to-Shell MVP, 2026-05-15). Since then, validated **on real AMD Zen** unless noted — the latest iron burn is agnos **1.57.1** (2026-09-08); 1.57.2 → 1.57.10 are built and QEMU-gated but **not burned**:

- **Storage** — NVMe / SATA / USB-MS stack, iron-validated.
- **Networking** — full TCP/IP + DHCP + DNS + NTP + ICMP over an r8169 NIC, iron-validated.
- **Filesystems** — read+write ext2/ext4 (incl. **ext4 extent allocation** + **JBD2 crash-safe journaling**), FAT12/16/32, exFAT; FS-crash-safety confirmed on hardware.
- **exec-from-disk** — static programs load + run in ring 3 off the agnos-fs, iron-validated.
- **Userland shell** — the interactive shell is the userland **agnoshi** binary, exec'd from disk in ring 3 (the in-kernel shell is now a recovery-only REPL, locking the kernel↔userland boundary); shell-separation arc **iron-validated** on real Zen. First AGNOS-tic tools (`bnrmr`/`cmdrs`/`klug`/`anuenue`) live on `/bin`, run via `run /bin/<tool>`.
- **Graphics + DOOM** — a framebuffer / timing / input path (`fbinfo`/`blit`/`uptime_ms`/`sleep_ms`/`kbscan`) culminating in **DOOM (cyrius-doom) exec'd from disk in ring 3** — the first real userland app, **iron-validated** on real hardware (plays in-game, keyboard-driven).
- **Multi-threading + preemptive scheduling + SMP** — the kernel moved from cooperative single-core round-robin to **preemptive ring-3 time-slicing**: concurrent ring-3 processes (each on its own CR3, all syscalling), a ring-3 **parent** that `spawn`s a child ELF + poll-`waitpid`s it entirely from ring 3, and multi-core SMP — **iron-validated** on real Zen.
- **Audio** — an HDA/Azalia driver + ring-3 `snd_*` syscall band, culminating in **DOOM with sound out the analog front jack** (1.52.x) — **iron-validated** on real Zen.
- **FP/SIMD** — per-process XMM state (SSE-enable, per-proc FXSAVE, lazy `#NM` save/restore) delivering **real f64 in ring 3** (1.53.x) — **iron-validated** on real Zen.
- **GPU compute** — the AMD Cezanne iGPU brought up from nothing (PSP firmware load, GFXHUB, a mapped compute queue, hand-assembled gfx90c shaders) with **no amdgpu and no ROCm**, culminating in f64 matmul on the shader cores **bit-identical to the CPU reference including rounding**, dispatchable from ring 3 (1.54.x) — **iron-validated** on real Zen.
- **Display + power** — a sovereign DCN 2.1 scanout path (flip, vblank pacing, tear-free double-buffered present), a **sovereign ATOM BIOS interpreter** whose encoder and transmitter runs emit exactly the vendor oracle's write sequences on iron (a *dry* build — no MMIO, no amdgpu: it proves the emission matches the vendor, not a hardware effect; the *wet* transmitter drive — ATOM `#76` `DIG1TransmitterControl` ENABLE via `/bin/modeset --transmitter`, 1.56.13/14 — is a separate run and was never itself oracle-diffed), hardware 2D via CP-DMA, and **AGNOS powering itself off** through a full ACPI S5 sequence (1.55.x) — **iron-validated** on real Zen.
- **3D raster + native scanout** — a GPU triangle rasteriser (167 hand-authored gfx90c instructions, 20 of 20 cases byte-identical to the CPU reference), barycentric RGBA interpolation, perspective-correct texturing, depth test, and a kernel-side DCN modeset that puts the console at **native 2560x1440** with the scaler bypassed and a hardware pan (1.56.0–1.56.38) — **iron-validated** on real Zen.
- **Sovereign desktop** — on 2026-08-03 at `cpus online: 4`, the native **aethersafha** compositor hosted **two real client windows** — a present probe and **crab**'s dual-pane file manager — composited on the panel, 278 frames, keys delivered to the client, clean quit — **iron-validated** on real Zen. Since then, also on iron: the clients ride the kernel channel band instead of TCP-on-loopback (2026-08-07), every frame layer is composited on the GPU with no CPU blit (2026-08-08), a USB-mouse pointer and a titlebar drag (2026-08-09), and per-window opacity (2026-08-16). ⚠ Speed is the open problem: **~7–15 fps measured on iron 2026-08-22** ([aethersafha `state.md`](https://github.com/MacCracken/aethersafha/blob/main/docs/development/state.md)). Not Wayland: the desktop speaks the sovereign **setu** protocol.
- **Processes** — process enumeration (`#99 proclist`, 1.56.47; first ring-3 call on iron 2026-09-03) and `fork` (`#96`, 1.56.54, corrected 1.56.55) with a wait-any `waitpid`. `fork` is QEMU-gated; no iron run of it is on record.
- **The blocking model** (1.57.6–1.57.9) — every process runs its syscalls on its own kernel stack, and every wait blocks only its caller (keyboard, pipe and channel reads; pipe writes); `kill` ends, stops and continues a child; one-shot memory/CPU caps for a child (`#107`); TCP/UDP ids owned by the process that opened them — **built and QEMU-gated, not burned**.
- **IOMMU** — VT-d translation turns on for the first time at 1.57.10 — **QEMU-only** (q35 + `intel-iommu`), not burned. The AMD iron target runs with AMD-Vi disabled, so VT-d does not run there.
- **Console fonts** — vendored from **kashi** (parallel-agent-developed sibling repo). Since 1.57.2 the kernel also embeds a proportional TrueType face (Liberation Sans, via **rekha**) that a client can `open()` at `/fonts/default.ttf` — built and QEMU-gated, not burned.

40+ subsystems ported from Rust to Cyrius. Base kernel-internals are complete — the GPU, display and desktop arcs sit on top of them. No Linux dependency at runtime — the kernel exposes a **small sovereign syscall surface with no BSD socket family, no `socket()` over arbitrary domains, no splice and no AF_ALG** (structurally immune to that CVE class). Live binary sizes, per-repo versions, syscall count, and cycle state track in the ecosystem state ledger.

AGNOS is a general, sovereign OS that stands on its own — kernel, shell, tools, and network all work with zero AI. It's **AI-native across the system**, but never AI-bound: the intelligence layer is one you turn on, off, or shape into whatever you want it to be — a feature, not a mandate.

> *A worthy substrate isn't built for web apps. It's built to be a home for whatever intelligence may arrive — or for none.*
>
> *A system that can't prove its own integrity can't be trusted with autonomous action.*

---

## Architecture

```
+------------------------------------------------------------------+
|  Desktop (on iron)      |  Agent Runtime          |  Kernel       |
|  +--------------------+ |  +--------------------+ |  +----------+ |
|  | aethersafha        | |  | daimon             | |  | AGNOS    | |
|  | native compositor  | |  | MCP tool host      | |  | kernel   | |
|  |                    | |  | Agent orchestrator | |  | 40+ sub- | |
|  +--------------------+ |  +--------------------+ |  | systems  | |
|  | agnoshi            | |  | hoosh              | |  | sovereign| |
|  | AI shell           | |  | LLM gateway        | |  | syscalls | |
|  |                    | |  | multi-backend      | |  | Cyrius-  | |
|  +--------------------+ |  +--------------------+ |  | native   | |
|  | consumer apps      | |  | aegis + sigil      | |  | iron-    | |
|  | marketplace (mela) | |  | kavach (sandbox)   | |  | validated| |
|  |  (reference)       | |  |                    | |  |          | |
|  +--------------------+ |  +--------------------+ |  +----------+ |
+------------------------------------------------------------------+
```

## The Stack

| Layer | Component | Notes |
|-------|-----------|-------|
| **Kernel** | AGNOS | Cyrius-native, 40+ subsystems, iron-validated NUC AMD 2026-05-15 |
| **Compiler** | Cyrius | self-hosting from 29KB seed |
| **PID 1** | kybernet | service supervision, signal/event-loop |
| **Init** | argonaut | 3 boot modes (Server / Desktop / Minimal) |
| **Sandbox** | kavach | Landlock + seccomp-bpf on Linux targets; on AGNOS both backends report *not supported* |
| **Crypto** | sigil | Ed25519, trust verification |
| **Audit** | libro | Hash-chained event log |
| **MCP** | bote | message pipeline + host registry |
| **LLM** | hoosh | OpenAI-compatible proxy, multi-backend, hardware accel |
| **Agents** | daimon | agent orchestrator, MCP tool host |
| **Shell** | agnoshi | natural-language terminal |
| **Packages** | ark + nous | package manager + resolver |
| **Recipes** | zugot | `.cyml` recipes, plus the **bazaar** community overlay — live counts in each repo's README |

> v1.0+ stable libraries: [`docs/applications/libs/README.md`](docs/applications/libs/README.md); v1.0+ binaries & tools: [`docs/applications/binaries.md`](docs/applications/binaries.md).

## Port Receipts (Rust to Cyrius)

| Subsystem | Before | After | Ratio |
|-----------|--------|-------|-------|
| agnodrm (was agnosys) | 6.9MB | 117KB | 59x smaller |
| kybernet | 6.7MB | 486KB | 14x smaller |
| hoosh | 5.1MB | 474KB | 10.8x smaller |
| kavach | 2.4MB | 344KB | 7x smaller, 500x faster lifecycle |
| ai-hwaccel | 708KB | 217KB | 3.3x smaller, 518 tests |
| avatara | — | — | 2,761x faster cached access |

## Consumer Apps (planned / reference)

The desktop tier itself is live — the native **aethersafha** compositor hosts real client windows on real hardware (2026-08-03). The consumer-app roster below is the planned marketplace roster, targeted to ship as `.agnos-agent` bundles — a roadmap, not a status: several already exist as standalone repos (**AgnosAI**, **Irfan** — repo `ifran` — and **Jalwa** as Cyrius ports; **SecureYeoman** in TypeScript), with current status in each repo:

| App | Description |
|-----|-------------|
| **SecureYeoman** | Sovereign AI agent platform (flagship) |
| **AgnosAI** | Agent orchestration engine |
| **Irfan** | LLM management and training |
| **Delta** | Code hosting (git, PRs, CI/CD) |
| **Aequi** | Self-employed accounting |
| **Jalwa** | AI-native media player |
| **Tazama** | AI-native video editor |
| **Shruti** | Digital audio workstation |
| **Rasa** | AI-native image editor |
| **Mneme** | AI-native knowledge base |
| **Photis Nadi** | Productivity app |
| **Nazar** | System monitor |
| And 11 more... | |

## Development Status

**Pre-beta.** Critical path to boot cleared. No closed-beta opening is recorded yet — see the targets below.

| Milestone | Status |
|-----------|--------|
| Sovereign kernel (40+ subsystems, iron-validated NUC AMD 2026-05-15) | Done |
| Kernel perf + hardening (heap-zero perf, page-map/RBP/reap hardening) + sysinfo/klog syscalls (`uname`/`sysinfo`/`klog`) | **QEMU-validated; rode later burns, no per-arc iron PASS recorded** (1.42.x — the tracker is still open as a burn-prep ride-along. The `klog`#36 reader `klug` is in routine use as the standard iron burn readout) |
| Graphics path + first real userland app — DOOM (cyrius-doom) exec'd from disk in ring 3 | **Iron-validated** (plays in-game on real Zen) — 1.43.x |
| Preemptive ring-3 multi-threading + SMP (per-proc CR3, time-slicing, concurrent exec + clean exit, real ELF spawn) | **iron-validated on real Zen** (1.44.x arc; SMP/preempt iron-confirmed 1.46.x) |
| HDA audio — DOOM with sound out the analog front jack | **Iron-validated on real Zen** — 1.52.x |
| Kernel FP/SIMD — real f64 in ring 3 (per-proc XMM state, lazy `#NM` save/restore) | **Iron-validated on real Zen** — 1.53.x |
| Kernel GPU compute — hand-assembled gfx90c shaders on the AMD iGPU with no amdgpu and no ROCm; f64 matmul bit-identical to the CPU reference, dispatchable from ring 3 | **Iron-validated on real Zen** — 1.54.x |
| Display + ACPI — DCN 2.1 scanout, vblank-paced tear-free present, sovereign ATOM BIOS interpreter, CP-DMA hardware 2D, self-poweroff via ACPI S5 | **Iron-validated on real Zen** — 1.55.x |
| GPU 3D raster + native scanout — triangle rasteriser (20/20 byte-identical to the CPU reference), perspective-correct texturing, depth test; kernel-side DCN modeset to native 2560x1440 with a hardware-panned console | **Iron-validated on real Zen** — 1.56.0–1.56.38 |
| **Sovereign desktop on iron — aethersafha hosts two real client windows** (a present probe + crab's dual-pane file manager) composited on the panel at `cpus online: 4`, 278 frames, keys delivered, clean quit | **Iron-validated on real Zen 2026-08-03** — 1.56.35 |
| Local display IPC — a kernel channel band, replacing TCP-on-loopback (retired 2026-08-03 as the wrong primitive for a local protocol) | **Iron-validated on real Zen 2026-08-07** — band shipped 1.56.40, first burned in the 1.56.41 kernel (through 1.56.40 this row read *not yet burned*) |
| Desktop on the GPU — every frame layer composited on the GPU, USB-mouse pointer + titlebar drag, per-window opacity | **Iron-validated on real Zen 2026-08-08 … 08-16** — 1.56.41–1.56.45. ⚠ ~7–15 fps measured on iron 2026-08-22 |
| Process enumeration + `fork` (`#99`, `#96`), wait-any `waitpid` | `#99` **run on iron 2026-09-03** (1.56.60); `fork` **QEMU-gated only** — 1.56.47–1.56.55 |
| The blocking model — per-process kernel stacks, every wait blocks only its caller, `kill` that ends/stops/continues, per-child caps, owned sockets | **Built + QEMU-gated, NOT burned** — 1.57.6–1.57.9 |
| VT-d IOMMU translation | **QEMU-only** (q35 + `intel-iommu`), NOT burned — 1.57.10; the AMD iron target runs with AMD-Vi disabled |
| Cyrius compiler (self-hosting from 29KB seed) | Done |
| 40+ subsystem ports (Rust to Cyrius) | Done |
| Sovereign boot pipeline (Cyrius) — sovereign UEFI handoff via gnoboot | Done |
| LFS base recipes (zugot, plus the bazaar community overlay) | Done |
| Security stack (kavach, sigil, libro, aegis at v1.0+) | Done |
| Consumer apps with MCP integration (marketplace roster) | **Planned** — no app ships as a marketplace bundle yet; several exist as standalone repos (see above) |
| **Self-hosting (AGNOS builds AGNOS)** | **Public-beta scope — not a closed-beta gate.** Kernel already builds + boots against current Cyrius; the bare-metal toolchain target landed in Cyrius **v6.2.x** and does not gate the MVP (the earlier "closed-beta blocker" framing was pre-monolith-extraction residue, corrected 2026-05-12) |
| Closed-beta tester cohort (5–15 trusted testers) | Pending closed-beta cut |
| Third-party security audit | Public-beta gate |
| Community testing program (formal enrollment) | Public-beta gate |

**Closed-beta target: late August 2026 — passed with no opening recorded; revised date not yet set** | **Public-beta target: deferred post-summer** | **GA target: late fall / early winter 2026**

See [CHANGELOG.md](CHANGELOG.md) for full details.

## Quick Start

### Build the boot pipeline (Cyrius)

```bash
git clone https://github.com/MacCracken/agnosticos.git
cd agnosticos/scripts
cyrius build src/boot.cyr build/boot
./build/boot --status
```

### Boot in QEMU

```bash
cd agnosticos
make boot-test
```

## System Requirements

| | Minimum (CLI) | Recommended (Desktop + LLMs) |
|---|---|---|
| **CPU** | x86_64 | 8+ cores |
| **RAM** | 4 GB | 32 GB+ |
| **Storage** | 20 GB SSD | 100 GB NVMe |
| **GPU** | -- | NVIDIA/AMD/Intel discrete |

## Security

- **kavach** — Landlock + seccomp-bpf sandboxing on **Linux targets**; on AGNOS both backends return a structured *not supported* (native confinement is intended, not built)
- **sigil** — Ed25519 trust verification, revocation
- **libro** — Cryptographic audit chain, hash-linked event log
- **aegis** — security library for threat detection, quarantine and scanning (its binary prints `aegis ready` and exits — it does not yet run as a daemon)
- **AGNOS kernel** — the kernel hardening track closed 13/13 at 1.28.0, but several of its controls do not run as named: KPTI collapsed at 1.40.3; the KASLR allocator half was removed at 1.41.12 and the image slide needs a build flag nothing sets; IBRS is CPUID-gated; and VT-d translation did not turn on in any boot until 1.57.10, and then in QEMU only (the AMD iron target runs with AMD-Vi disabled). Per-control status: [security-guide.md](docs/security/security-guide.md). Structurally immune to CVE-2026-31431 (no BSD socket family, no `socket()` over arbitrary domains, no `splice`, no AF_ALG in the sovereign syscall table)

See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## Documentation

| Document | Description |
|----------|-------------|
| [architecture.md](docs/architecture.md) | System architecture |
| [libs/README.md](docs/applications/libs/README.md) | v1.0+ stable library registry |
| [binaries.md](docs/applications/binaries.md) | v1.0+ binaries & tools registry |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines |
| [SECURITY.md](SECURITY.md) | Security policies |

## License

**GNU General Public License v3.0** (GPLv3). See [LICENSE](LICENSE).

Desktop GUI applications are **AGPL-3.0**.

---

<div align="center">

**AGNOS** -- The Operating System for the Age of AI

*Built for agents. Controlled by humans. Written in Cyrius.*

</div>
