# gnoboot

> **gnoboot** — AGNOS sovereign UEFI bootloader. Cyrius-native, replaces GRUB.

| Field | Value |
|-------|-------|
| Status | Released, pre-1.0 (handoff contract not yet frozen). Live version in [`VERSION`](https://github.com/MacCracken/gnoboot/blob/main/VERSION); Cyrius pin in its `cyrius.cyml`. |
| Repository | `MacCracken/gnoboot` |
| Runtime | UEFI Application (PE32+ EFI, x86_64) |
| Recipe | `zugot/marketplace/gnoboot.cyml` (since zugot 1.0.7, 2026-07-03 — consumed by agnova's bootloader phase). Iron media is provisioned by `agnosticos/scripts/install-media.sh` (`install-usb.sh` is now a forwarding shim). |
| MCP Tools | None (pre-kernel; no AGNOS userland to host MCP) |
| Agnoshi Intents | None |
| Pairs with | agnos ≥ 1.30.0 (sovereign-struct contract); ≥ 1.47.4 for the KASLR / `ET_DYN` load path. Current pairing: agnos [`state.md`](https://github.com/MacCracken/agnos/blob/main/docs/development/state.md) (Bootloader row). |

---

## Why First-Party

The AGNOS sovereignty pattern is: **own your stack**. Cyrius replaced gcc/clang/llvm; agnos replaced Linux; gnoboot replaces GRUB. Until gnoboot existed, AGNOS depended on a third-party bootloader whose multiboot2-EFI relocator broke under modern strict-W^X UEFI. gnoboot dissolves that dependency — AGNOS boots end-to-end on its own.

## What It Does

- UEFI firmware loads gnoboot at `\EFI\BOOT\BOOTX64.EFI`
- gnoboot opens `\boot\agnos` from the ESP via `SimpleFileSystemProtocol`
- Validates the ELF64 header up front, then loads **every** `PT_LOAD` into one contiguous allocation — at `p_paddr` for an `ET_EXEC` kernel (`0x100000` for agnos), or at a RDRAND-chosen KASLR base for an `ET_DYN` one (0.6.0). Every segment is bounds-checked against the file and short reads are rejected (0.7.0; before that only the first `PT_LOAD` was loaded, unchecked)
- Reads kernel segment data directly into place and zero-fills each BSS gap
- Optionally loads `\boot\initramfs` + `\boot\cmdline` and captures the ACPI RSDP (0.5.0); selects the largest RGB/BGR GOP mode the firmware offers (0.6.1)
- Builds the AGNOS sovereign boot-info struct: magic `'AGNO'` (`0x41474E4F`), wire version 2, a **flat, fixed-offset** struct with no tag stream — **128 bytes** (`struct_size` `0x80`) since 0.7.1, after 80 bytes at 0.1.0 (version 1), 112 at 0.2.0 and 120 from 0.4.0. It carries the memory map, EFI SystemTable, ACPI RSDP, optional initramfs/cmdline, the inlined framebuffer fields and `kernel_base`. Authoritative layout: gnoboot's [`docs/standards/handoff-protocol.md`](https://github.com/MacCracken/gnoboot/blob/main/docs/standards/handoff-protocol.md)
- Calls `bs->GetMemoryMap` (fresh `mm_key`), then `bs->ExitBootServices(handle, mm_key)`
- Jumps to kernel entry with `RDI = &boot_info` (sovereign-struct handoff contract)

After ExitBootServices, ConOut is gone; any further diagnostic comes from the kernel's own UART driver.

## Architecture

See the Path C plan for the original architecture and rationale; the authoritative handoff contract (registers, every field, compatibility rules, required consumer validation) is gnoboot's [`handoff-protocol.md`](https://github.com/MacCracken/gnoboot/blob/main/docs/standards/handoff-protocol.md) (written at 0.7.1; the freeze is a v1.0 criterion).

Key invariants:
- **`fn efi_main(handle, st)` entry convention** — cyrius 5.11.52+ auto-emits the firmware-arg-capture trampoline (RCX → R14, RDX → R15) around `gvar_inits` + the call
- **`lib/fnptr.cyr` MS-x64 dispatch** — under `CYRIUS_TARGET_EFI=1`, cyrius predefines `CYRIUS_TARGET_WIN` too, so `fncallN`'s MS-x64 ABI branches fire for firmware calls. Since cyrius 6.5.17 (gnoboot 0.6.2) the compiler lowers expression-position `fncallN` inline on the same MS-x64 path; the vendored `fncall2` still carries the three statement-position `SetMode` calls
- **Byte-array literal globals** — UTF-16LE strings and EFI GUIDs declared as `var foo[N] = { 0x.., ... };` (cyrius 5.11.51+)

## Verification

- **QEMU OVMF**: `tests/ovmf_smoke.sh` in the gnoboot repo. Builds a GPT-disk-with-ESP, boots under `qemu-system-x86_64 -cpu max -machine q35` + OVMF firmware. Verified continuously through every minor release.
- **Iron (NUC AMD archaemenid)**: iron-validated end-to-end. MVP gate cleared on real hardware (2026-05-18, agnos 1.30.9) — `agnos> echo "Assembly Up!"` echoed on iron USB Logitech keyboard. Storage iron debuts followed: NVMe (Crucial P3 2 TB), AHCI/SATA (WD Blue SA510 2 TB), USB-MS (Silicon Motion stick, full INQUIRY/TUR/RC10).

## Status

- ✓ MVP handoff verified on QEMU OVMF emulation (2026-05-13)
- ✓ Iron MVP gate cleared on archaemenid NUC AMD (2026-05-18)
- ✓ FB-handoff observability bundle (GOP mode capture, serial diagnostic, CMOS stamp) — 0.3.0 → 0.4.x
- ✓ AMD Zen Quiet-Boot scanout residue — parked gnoboot-side at the 0.4.2 closeout (2026-05-20), then **resolved kernel-side** at agnos 1.55.28 (2026-07-20): a surface/raster scale mismatch, not tiling — neither parked option (HUBP `clear_tiling` port, shadow buffer) was needed
- ✓ `boot_info` feature fill — initramfs, cmdline, ACPI RSDP (0.5.0, 2026-06-03); the kernel-side readers are agnos's work
- ✓ KASLR / `ET_DYN` kernel load (0.6.0, 2026-06-27) — ⚠ not yet runtime-exercised, since agnos links `ET_EXEC`
- ✓ Native-resolution GOP mode selection (0.6.1, 2026-08-03)
- ✓ First security audit + ELF-load hardening, multi-`PT_LOAD` (0.7.0, 2026-08-29)
- ✓ Handoff spec written, `boot_info` 120 → 128 bytes (0.7.1, 2026-08-29)
- ⏳ aarch64 UEFI port (Pi 4) — gnoboot v0.9.0
- 🔒 Handoff contract freeze at gnoboot v1.0.0

Full roadmap: [gnoboot/docs/development/roadmap.md](https://github.com/MacCracken/gnoboot/blob/main/docs/development/roadmap.md).

## Cross-references

- [agnos](agnos.md) (when added) — the kernel gnoboot loads. agnos 1.30.0 cut the sovereign-struct ABI break that originally paired with gnoboot v0.1.0; the current pairing lives in agnos `state.md` (Bootloader row), not here.
- [cyrius](https://github.com/MacCracken/cyrius) — toolchain. gnoboot's bring-up surfaced multiple cyrius issues that landed across v5.11.49 → v5.11.69 and into v6.0.x.
- Sovereign UEFI plan — full architecture
- Iron-boot test log — active iron boot log (post-MVP). gnoboot ships from early in the iron boot arc; full MVP-era arc is tracked in the iron boot log.

## Related ADRs (in gnoboot)

- [0001 — Sovereign boot-info struct over multiboot2](https://github.com/MacCracken/gnoboot/blob/main/docs/adr/0001-sovereign-struct-over-multiboot2.md)
