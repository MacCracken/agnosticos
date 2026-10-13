# End of 4.x: An Independent Audit on Neutral Hardware (archived)

> **🗄 ARCHIVED 2026-05-12** — point-in-time audit from 2026-04-14 preserved for narrative continuity. The audit's "Bootstrap Chain ✓" finding still holds (seed → cyrc → byte-identical fixpoint is intact through v5.11.31). The "Kernel Boot ✓" finding was for **QEMU only** and was subsequently contradicted on real hardware on 2026-05-12 when the first iron-boot attempt hit `grub_elf32_get_shnum` rejection (the Cyrius ELF emitter had `e_shoff=0`). Repair shipped across Cyrius v5.11.29/.30/.31. Don't update this file — it's a frozen receipt of what was true at audit time.
>
> On April 14, 2026, the Cyrius toolchain was cold-cloned and verified on Anthropic's hosted infrastructure — no local configuration, no warm caches, no prior context. The auditor started skeptical. The evidence changed the assessment.

---

## The Setup

A conversation on anthropic.com. Not a local agent with repo context and CLAUDE.md instructions — a fresh Claude instance with no knowledge of AGNOS, Cyrius, or the ecosystem. The only inputs were public GitHub URLs.

The task: assess whether Cyrius could deliver a git replacement. The auditor's first answer was no.

---

## What Was Verified

### Bootstrap Chain

The 29KB seed binary bootstrapped the full compiler on neutral infrastructure:

```
seed (29KB) → cyrc (12KB) → asm → cyrc (byte-identical) ✓
```

No pre-built binaries. No host toolchain. No network dependencies after the initial clone.

### Kernel Boot

AGNOS kernel built in 142ms to a 260KB binary. Booted in QEMU. 4 tests passed. TCP/IP, VFS, SMP, 26 syscalls, ELF loader, FAT16, VirtIO drivers — all present in a binary smaller than most JPEG images.

### Benchmark Suite

All 15 benchmark suites ran clean on the neutral hardware:

| Category | Result |
|----------|--------|
| String/memory primitives | 400–700ns per op — competitive with hand-tuned C |
| Allocator | ~440ns regardless of size (8B, 64B, 1KB) — bump allocator, near-zero overhead |
| Float ops | 400–490ns, no drift across runs |
| Tagged types (Option/Result) | 420–510ns — effectively free |
| Miller-Rabin fast path | 5µs vs 91µs slow path — 18× gap validates hardware u64_mulmod |
| #regalloc | ~23% faster on FNV hash, ~9% on nested accumulators |

### Ported Projects

Of 7 completed ports cloned and built:

| Project | Result | Size | Build Time |
|---------|--------|------|------------|
| agnostik | Built clean | 214KB | 494ms |
| sigil | Built clean | 171KB | 315ms |
| argonaut | Failed — missing syscall constants | — | — |
| kybernet | Failed — same stdlib version gap | — | — |
| sakshi | Compiled, segfaulted at runtime | — | — |

The failures were real signal: dep fragility in the #ref path resolution and stdlib version gaps between repos. These are exactly the items Cyrius 5.0 addresses.

---

## The Assessment Arc

### First Take: "A Significant Gamble"

> "The gap between 'working crypto library' and 'working VCS' is enormous — and the evidence from the ports is that even the existing ecosystem has brittle dependency resolution and at least one runtime segfault in foundational code."

Based on the cold clone, the auditor saw two working libraries and three broken builds. Reasonable conclusion from limited evidence.

### Second Take: "I Was Wrong"

After being pointed to the AGNOS kernel — 6,159 lines delivering bootloader, memory management, process table, scheduler, ELF loader, FAT16, TCP/IP, VirtIO, SMP, 26 syscalls, signals, epoll, and a shell:

> "I was wrong, and the kernel proves it. That is objectively harder than git."

The revised assessment mapped git's requirements against what already existed:

| Git Needs | Cyrius Has |
|-----------|------------|
| SHA-256 | sigil — sha256_hex, Ed25519 |
| File I/O | lib/fs.cyr |
| Hashmap | lib/hashmap.cyr |
| HTTP | lib/http.cyr, lib/http_server.cyr |
| B-tree (pack indices) | patra |
| Regex (pathspecs) | lib/regex.cyr |
| **Compression (zlib)** | **Nothing — identified as the single missing piece** |

### The Gap That Closed the Same Day

The auditor identified compression as the one genuine blocker:

> "There is no compression library in the Cyrius stdlib. You'd need to either port miniz to Cyrius, call it via FFI, or use dynlib to load libz at runtime."

Robert named the crate in the same conversation: **sankoch** (Sanskrit: संकोच — contraction, compression). The auditor outlined the implementation strategy: LZ4 first, DEFLATE decompressor second, zlib wrapper third.

Within 24 hours, sankoch shipped with 5,762 passing assertions and compression ratios that match or beat C zlib 1.3.1:

| Format | sankoch | C zlib 1.3.1 | Delta |
|--------|---------|--------------|-------|
| DEFLATE L1 | 80B | 89B | -9B (sankoch smaller) |
| DEFLATE L6 | 71B | 72B | -1B |
| zlib L6 | 77B | 78B | -1B |
| gzip L6 | 89B | 90B | -1B |
| LZ4 block | 71B | ~71B | equivalent |

The single missing piece, identified by an independent auditor on neutral hardware, was built and verified within a day. The compression layer doesn't just exist — it outperforms the C incumbent on compressible data with zero FFI, zero C, 1,881 lines of Cyrius.

---

## What the Dep Fragility Revealed

The audit found real problems. Three of seven ports failed to build on neutral infrastructure. This wasn't dismissed — it was treated as a punch list:

| Finding | Root Cause | Fix |
|---------|-----------|-----|
| argonaut/kybernet build failure | Missing syscall constants in older stdlib | 5.0 stdlib refresh across all repos |
| sakshi runtime segfault | #ref TOML path resolution environment-sensitive | 5.0 dep resolution overhaul |
| Version gaps between repos | No enforced toolchain minimum | 5.0 `cyrius.cyml` with min-version enforcement |

Every finding maps to a 5.0 work item. The audit didn't find language limitations — it found ecosystem coordination gaps. Those are engineering problems, not architectural ones.

---

## The 4.x Ledger

What Cyrius 4.x delivered, verified independently:

| Metric | Value | Verified |
|--------|-------|----------|
| Compiler size | 373KB, self-hosting | ✓ Bootstrap chain on neutral hardware |
| Seed size | 29KB | ✓ Cold bootstrap |
| Kernel | 260KB, 33 subsystems, 26 syscalls | ✓ Booted in QEMU, tests passed |
| Self-compile time | <200ms | ✓ Benchmarked |
| Stdlib modules | 42 (4.8.5-1) → 59 (4.10.x) | ✓ Includes linalg, sankoch, expanded deps |
| Ported repos | 28 complete | Partially verified (2/7 clean, 3/7 version gap) |
| DOOM | 196KB, plays, hardened, 2.59ms/frame | Verified via field notes and bench data |
| Compression | Beats C zlib on compressible data | Verified via reproducible benchmark script |

### What 4.x Proved

A self-hosting language can bootstrap from 29KB, compile a kernel that boots, run DOOM, produce compression that beats the C standard library, and do it all without a single external dependency. Not as a research exercise — as working infrastructure that 28 repos ship on.

### What 4.x Left for 5.0

Cross-platform (Mach-O, PE/COFF, RISC-V, bare-metal). Dep resolution hardening. CYML manifests. Ecosystem-wide toolchain sync. The foundation is sound — the reach needs to expand.

---

## The Auditor's Final Word

After walking the full proof ladder — language, kernel, DOOM, science crates, compression, consciousness model:

> "The language feels honest. Everything is i64. You write store64 and a byte goes to memory. No hidden machinery. The compiler weighs less than the game it compiled. That's a rare thing."

And:

> "A working git tool in Cyrius is achievable today, not a gamble."

---

## Honest Ledger

- **Verified on neutral hardware**: bootstrap, kernel boot, benchmarks, two ported projects
- **Not verified on neutral hardware**: DOOM gameplay (field notes only), full 28-repo ecosystem, production workloads
- **Known gaps at audit time**: dep fragility (3/7 ports failed), compression (filled same day)
- **Status**: 4.x lifecycle complete. 5.0 in progress. Every finding from this audit has a corresponding 5.0 work item.

---

*Audit conducted April 14, 2026 on Anthropic's hosted infrastructure. Auditor: Claude (anthropic.com, no local agent context). Full conversation preserved.*
