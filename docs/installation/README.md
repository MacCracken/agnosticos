# AGNOS Installation Guide

> **Last Updated:** 2026-09-27
>
> **Status:** AGNOS is **Pre-Beta**. Closed beta was targeted to open late August 2026 (after a founder solo-dogfood month + the server-stage weak-point sweep, windowed ~July / early-Aug 2026; the sweep's harness is **TBD** and explicitly **not** Docker — the QEMU-in-a-Docker-container design was retired 2026-07-07). **That date has passed with no record of the cohort opening**, and its gate, the ISO Stage-4 cut, is not started — current targets live on the [roadmap](../development/roadmap.md). Public beta is deferred to post-summer, with GA targeted late fall/early winter 2026. The kernel boots both in QEMU and on real AMD Zen hardware, and the sovereign boot pipeline is active. There is still **no end-user install path**: the distributable image (the **ISO Stage-4 cut**) and `agnova`'s native installer are both open stage-exit work. This guide describes what is buildable and testable **today**, and what is coming.

---

## What Works Today

- **Sovereign boot pipeline** — `scripts/src/boot.cyr` (Cyrius): ISO component verification plus a QEMU launcher. ⚠ The launcher's direct `-kernel` path predates agnos 1.30.0 and cannot load the current kernel — see [QEMU Boot Test](#qemu-boot-test-kernel). Compiled size drifts every cut; `make status` prints the live figure.
- **AGNOS kernel** — Cyrius-native, 40+ subsystems, a small sovereign syscall surface (no BSD socket family, no `socket()` over arbitrary domains, no splice, no AF_ALG). Boots on real AMD Zen (the NUC devbox, archaemenid) to the ring-3 userland shell **agnsh**, exec'd from the agnos-fs. ⚠ Iron validation is **per-cut, not blanket** — the head is often an unburned cut, and not every released cut has been burned. Per-cut burn status: [`../development/state.md`](../development/state.md).
- **Iron boot media** — `scripts/install-media.sh` (the old `install-usb.sh` is now a forwarding shim) provisions a USB stick or an internal disk with gnoboot's ESP + the kernel + an ext4 `agnos-fs` root, and refreshes either without a wipe. Iron-proven on archaemenid — but it is a developer provisioning tool, **not** an end-user installer.
- **Component verification** — `make iso-check` walks every downstream repo and confirms the build artifacts an ISO would need are present, printing each repo's `VERSION` (it does not compare versions).
- **Per-subsystem builds** — every subsystem (kybernet, ark, nous, sigil, libro, agnoshi, …) builds standalone from its own repo via `cyrius build`.

## What is Coming

- **ISO Stage-4 cut** — package the iron-proven boot media as a distributable writable `.img` with a writable ext4 rootfs. Decisions are **LOCKED**; `scripts/src/iso.cyr` is **not started but unblocked**. Plan: [`iso-stage4-plan.md`](../development/iso-stage4-plan.md). The LFS-style Stages 1–3 (source download, cross-toolchain bootstrap, chroot base-system build) are deferred behind it; the read-only `.iso` is a follow-on gated on a kernel-side initramfs reader (gnoboot has loaded `\boot\initramfs` into `boot_info` since 0.5.0; the kernel only logs the field so far).
- **agnova** (installer) — Cyrius-native, past the scaffold stage, but a finished *port* is not a finished *installer*: the native installer capability is still owed to the server stage. Will own disk partitioning, LUKS2, bootloader install, profile selection (Desktop / Server / Minimal). ⛔ Hard prerequisite chain: sovereign ark → agnova → server-stage exit.
- **takumi** (build system) — the Cyrius port is **done**; what is still coming is its use in ISO assembly, driving recipe builds out of zugot.
- **Target**: closed beta cut, **late August 2026** — passed with no record of the cut (see Status above). ⚠ It is **not** gated on the Cyrius toolchain — the bare-metal target shipped at Cyrius **v6.2.x**, and the kernel already builds and boots against the current pin. The remaining dependency is agnosticos-side: the ISO Stage-4 cut plus a first non-founder boot session.

---

## Requirements (target hardware for Beta)

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | x86_64 or aarch64, 2 cores | x86_64, 4+ cores; or Raspberry Pi 4/5 (aarch64) |
| RAM | 2 GB | 8 GB (16 GB for local LLM workloads) |
| Disk | 20 GB | 100 GB NVMe SSD |
| Boot | UEFI (GPT + FAT ESP) — Legacy BIOS **not supported** | UEFI with Secure Boot |
| GPU | — (CLI/server) | NVIDIA, AMD, or Intel integrated (for desktop) |

These are **targets for the first bootable image**, not today's support matrix — only x86_64 boots right now; the kernel's aarch64 tree **does not currently compile** (found at agnos 1.56.51, 2026-08-28 — its test probe had been skipping it silently since cyrius v6.1.1 dropped the `cc5_aarch64` name it probed for) and has no boot harness. Full hardware matrix: [system-requirements.md](system-requirements.md).

---

## Building and Testing Today

All commands run from the genesis repo root unless noted.

### Prerequisites

Cyrius toolchain (required):
```sh
# Install from release tarball or build from source (cyrius repo)
# Toolchain version pinned in scripts/cyrius.cyml (manifest is single source of truth)
which cyrius  # verify on PATH
```

Host build tools (for QEMU):
```sh
# Debian/Ubuntu
sudo apt install qemu-system-x86 qemu-utils

# Arch
sudo pacman -S qemu-full
```

### Build the Sovereign Boot Pipeline

```sh
cd scripts
cyrius build src/boot.cyr build/boot
./build/boot --help
```

The `boot` binary is the Cyrius-native launcher used by every `make boot-*` target.

### Component Verification

```sh
make iso-check
```

Walks every downstream repo (agnos, kybernet, ark, nous, sigil, libro, …) and verifies the build artifacts an ISO would need are present, printing each one's `VERSION`. Current ISO-pipeline entry point — **Stage 0**.

### QEMU Boot Test (Kernel)

```sh
make boot-test
```

Direct-boots the AGNOS kernel in QEMU via `boot.cyr` (`qemu -kernel`). No rootfs, no userland — so it would reach the kernel's **recovery-only** in-kernel REPL, not the interactive shell (**agnsh** is a ring-3 binary and needs a rootfs to exec from).

⚠ **This path cannot boot the current kernel.** `-kernel` is the pre-1.30.0 multiboot path. Since agnos **1.30.0** the kernel is an ELF64 image entered only through gnoboot's sovereign handoff (`RDI = &boot_info`), and QEMU refuses it outright: `Error loading uncompressed kernel without PVH ELF Note` (checked 2026-09-27 against agnos 1.57.10). Until `boot.cyr` gains a gnoboot + OVMF path, smoke-boot the kernel under QEMU + OVMF through gnoboot — a timed, non-interactive run that asserts gnoboot's hand-off banner:

```sh
# needs OVMF (edk2-ovmf), parted and mtools; build gnoboot first:
#   CYRIUS_TARGET_EFI=1 cyrius build src/main.cyr build/BOOTX64.EFI
cd ../gnoboot && sh tests/ovmf_smoke.sh   # AGNOS_KERNEL defaults to ../agnos/build/agnos
```

The kernel's own QEMU gates are the agnos repo's smoke battery (`scripts/smoke/`, driven by `scripts/sweep.sh`).

### Build a Specific Subsystem

Each subsystem lives in its own repo; build from there. Example:

```sh
cd /home/macro/Repos/kybernet
cyrius build src/main.cyr build/kybernet
```

Repo map: see the public map in [`docs/architecture.md`](../architecture.md).

---

## Installer and ISO — Status

The installer (`agnova`) and the bootable ISO do not exist yet as user-facing artifacts. They are tracked as **open stage-exit work** on the [roadmap](../development/roadmap.md) — the ISO Stage-4 cut is the base→server blocker, and agnova's native installer is a server-stage exit item. (The roadmap's old Phase 13A–24 numbering was deleted 2026-08-01; cite a maturity-arc stage or a named spec, never a phase number.)

Progress against the ISO pipeline:

| Stage | What it does | Status |
|-------|--------------|--------|
| 0 | Resolve components (verify artifacts across all repos) | **Implemented** — `make iso-check` |
| 1 | Download / vendor upstream sources | Deferred behind the Stage-4 cut |
| 2 | Bootstrap cross-toolchain from source | Deferred behind the Stage-4 cut |
| 3 | Build base system in chroot | Deferred behind the Stage-4 cut |
| 4 | Package the boot media into a distributable image | **Not started, unblocked** — `scripts/src/iso.cyr`, decisions LOCKED |

The **arch-neutral boot pipeline** (`boot.cyr` arch detection, per-arch branch tables, arch-aware ISO stages keyed on the target triple) is a medium-priority roadmap item, not a gate on any of the above. Cyrius's bare-metal target shipped at **v6.2.x**; RISC-V rv64 has **not** shipped and is pinned to Cyrius **v6.7.x / v6.8.x**.

---

## Troubleshooting

### Boot Pipeline

| Symptom | Cause / Fix |
|---------|-------------|
| `cyrius: command not found` | Toolchain not on PATH. Check `~/.cyrius/bin/` or install from cyrius release tarball. |
| `cyrius build` fails with missing stdlib | Running `cycc` directly; always use `cyrius build` (auto-prepends includes). |
| `make boot-test` fails: `Error loading uncompressed kernel without PVH ELF Note` | Expected since agnos 1.30.0 — `boot.cyr`'s `-kernel` path cannot load the ELF64 kernel. Boot it through gnoboot + OVMF (see [QEMU Boot Test](#qemu-boot-test-kernel)). |
| `make boot-test` hangs on black screen | QEMU serial not wired; check with `qemu-system-x86_64 --version` ≥ 7.0. |
| `make iso-check` reports `MISS … [no build artifact]` | Sibling repo hasn't been built (iso-check checks presence, not freshness). `cd ../<repo> && cyrius build …` then retry. |

### Kernel in QEMU

```sh
# boot.cyr always wires QEMU's serial console to stdio — there is no --serial flag
# (it exits with "Unknown option"); -v / --verbose is accepted. Its -kernel path
# cannot load agnos >= 1.30.0, so hand the kernel off through gnoboot + OVMF
# (this asserts gnoboot's hand-off banner, not a kernel boot):
cd ../gnoboot && sh tests/ovmf_smoke.sh
```

### Getting Help

- **Issue Tracker**: https://github.com/MacCracken/agnosticos/issues
- **Security Issues**: see [SECURITY.md](/SECURITY.md)
- **Roadmap**: tracked in the project's development docs

---

## Historical Note

Prior versions of this document described a Debian-based rootfs + `userland/` Cargo workspace + Docker distribution path. That toolchain was retired during the monolith extraction (2026-04-01) and the Cyrius pivot (2026-04-04). Scripts from that era live in `scripts/archive-pre-cyrius/` for reference only and do not build.

---

*See also: [CONTRIBUTING.md](/CONTRIBUTING.md) for development setup, [troubleshooting.md](troubleshooting.md) for common issues, [system-requirements.md](system-requirements.md) for the full hardware matrix.*
