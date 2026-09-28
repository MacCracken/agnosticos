# AGNOS Build Scripts

> Sovereign boot pipeline — written in Cyrius, compiled by `cycc`, zero bash dependencies.

## Quick Start

```bash
cd scripts/
cyrius build src/boot.cyr build/boot     # compile boot tool

# Interactive boot (default — fastest for development)
./build/boot

# Boot + validate AGNOS banner in serial output
./build/boot --test

# Show component versions and sizes across the sibling-repo tree
./build/boot --status

# Verify every component required for ISO assembly is present
./build/boot --iso-check

# Override the kernel path
./build/boot --kernel /path/to/agnos
```

## Programs

| Program | Purpose |
|---------|---------|
| `boot.cyr` | Assemble and boot AGNOS from sibling-repo components (kernel, toolchain, recipes). Sub-modes: `--test`, `--status`, `--iso-check`. |
| `read-boot-log.cyr` | Sovereign CMOS boot-log reader — reads the kernel's boot-stage checkpoint slots from RTC/CMOS by direct port I/O on the dev host (the Linux `/dev/nvram` driver rejects archaemenid's layout). |

## CLI

```
boot [OPTIONS]

Options:
  --test           Boot + validate AGNOS banner in serial output
  --status         Show component versions and sizes
  --iso-check      Verify all ISO components are present
  --kernel PATH    Path to AGNOS kernel binary
  -v, --verbose    Verbose output
  -h, --help       Show help
```

## How It Works

> ⚠ **The interactive and `--test` modes cannot boot a current kernel.** Both hand the kernel to QEMU `-kernel`,
> and since agnos **1.30.0** the kernel is an ELF64 image with no PVH note, so QEMU refuses it
> (`Error loading uncompressed kernel without PVH ELF Note`). agnos boots through **gnoboot + OVMF** instead —
> see agnos's README and `scripts/smoke/`. `--iso-check` still works. `--status` still runs, but its toolchain
> line reads `../../cyrius/build/cc5`, a binary cyrius stopped producing when `cc5` became `cycc` (`src/boot.cyr`
> still names it). It reports the tool missing — or, where an old `cc5` survives in that directory, prints the
> stale binary's size beside the current `VERSION` (2026-09-28: `cyrius v6.6.9 (874232 bytes)`, from a June `cc5`).
> It also still lists `agnosys`, which became `agnodrm` on 2026-06-19, and shows it as `---`.

**Interactive boot** (default, fastest for development):
```
../../agnos/build/agnos → QEMU -kernel → serial output (Ctrl-C to quit; plain -serial stdio has no Ctrl-A escape)
```

**Test boot** (`--test`):
```
../../agnos/build/agnos → QEMU -kernel → capture serial → search for "AGNOS" banner
```

**ISO check** (`--iso-check`): walks every required and optional subsystem
in the sibling-repo tree and reports `READY / MISS / SKIP` per component.

Full image assembly is **ISO Stage-4** work — a writable GPT `.img` with a FAT EFI
System Partition (gnoboot's `BOOTX64.EFI` + kernel) and an ext4 rootfs, cut by a planned
`src/iso.cyr` that is **not yet written** (plan: [`../docs/development/iso-stage4-plan.md`](../docs/development/iso-stage4-plan.md)).
AGNOS does not use GRUB, ISO9660/xorriso or squashfs — `--iso-check` no longer checks for them.
`--iso-check` is the gate.

## Component Sources

| Component | Default Path | Override |
|-----------|--------------|----------|
| AGNOS kernel | `../../agnos/build/agnos` | `--kernel PATH` |
| Cyrius toolchain | `../../cyrius/build/cycc` | — (sibling repo) |
| Subsystems | `../../<name>/build/<name>` | — (sibling repo) |

Paths are relative to `scripts/` — the boot tool runs with `scripts/` as
its working directory and climbs `../../` to reach sibling repos.

## Testing

```bash
cyrius test tests/boot.tcyr
```

Tests cover path utilities, file-existence checks, vec-based arg building,
and the AGNOS-banner substring search pattern. They do not spawn QEMU;
full boot is an integration test.

## Archive

Pre-Cyrius bash scripts preserved at `archive-pre-cyrius/` for reference.
These were written for the Rust era (cargo build, Debian debootstrap,
Linux 6.6 kernel). They are not maintained and should not be used for
new work.

## License

GPL-3.0-only — Part of [AGNOS](https://agnosticos.org)
