# Security Testing Checklist

**Last Updated**: 2026-09-27

> **How to read this file.** Every `- [ ]` box below is checkable **on AGNOS today** with a command or
> file you can actually run or open. Nothing here is aspirational. Controls that do not exist are not
> silently missing — they live in two no-tickbox sections at the bottom:
> **⛔ Not Applicable by Design** (AGNOS has no such subsystem and, per
> [`docs/development/roadmap.md`](../development/roadmap.md), never will) and
> **📋 Not Available Yet** (intended, unbuilt).
>
> ⚠️ **A checklist is the most dangerous fossil form**: an operator ticks a box and believes they are
> covered. The 2026-03 revision of this file carried tickboxes for Landlock, seccomp, namespace
> isolation, cgroup limits, capability checks, a permission system, PAM-shaped password policy, mTLS
> and ML-KEM key exchange. **None of those exist on AGNOS.** Worse, several of them produce
> *evidence-shaped emptiness* — an operator who greps for a seccomp filter and finds nothing reads
> "hardened" where the truth is "no mechanism". Those items were not left unchecked; they were moved
> below and labelled, because telling you *why the box is gone* is the deliverable.
>
> Marker convention matches root [`SECURITY.md`](../../SECURITY.md): 📋 intended / not built,
> ⚠️ caveat that changes what a green result means, ⛔ hard stop.

**Verified against**: agnos 1.57.10 (built and gated in QEMU, **not burned** — the last iron burn is
1.57.1) · cyrius 6.6.6 · gnoboot 0.7.2 · ark 1.4.2 · sigil 3.13.2 · libro 2.10.3 · aegis 1.1.8 ·
phylax 1.2.7 · kavach 3.13.1 · takumi 1.1.2 · zugot 1.0.7 · bote 3.3.13 · daimon 2.4.3 · hoosh 2.7.1 ·
mela 1.0.3 · shakti 0.7.2 (pre-1.0) — re-verified 2026-09-27. First written 2026-08-05 against agnos
1.56.40 · cyrius 6.5.7 · gnoboot 0.6.1 · ark 1.4.1 · sigil 3.12.2 · libro 2.8.4 · aegis 1.1.4 ·
phylax 1.2.4 · kavach 3.11.7 · takumi 1.1.1 · bote 3.3.0 · daimon 2.0.0 · hoosh 2.6.0 · mela 1.0.1 ·
shakti 0.7.0.

---

## The one-paragraph posture this checklist tests against

> **AGNOS today is a single-owner, single-trust-domain OS.** Every process runs as uid 0 —
> `agnos/kernel/core/syscall.cyr:9667` is literally `if (num == 15) { return 0; } # getuid (root)`.
> There is no uid model, no per-process capability model, and no kernel-backed confinement. Its
> security value is a radically small, sovereign attack surface that structurally lacks whole classes
> of Linux vulnerability — **not** the enforcement of boundaries between processes, users, or code of
> differing trust. Test it as what it is. A finding of "process A read process B's shared memory" is
> **documented behaviour**, not a vulnerability (`agnos/kernel/core/syscall.cyr:10284-10297`).

⚠️ **Repo paths.** This document lives in `agnosticos`. Paths prefixed `agnos/`, `ark/`, `sigil/`,
`kavach/` etc. are **sibling repos** under `/home/macro/Repos/`, not files in this one. Unprefixed
paths are relative to this repo.

---

## Pre-Engagement

⚠️ AGNOS is a single-maintainer pre-Beta project. There is no security team, no external tester roster,
and no NDA process. The 2026-03 revision's third-party-engagement framing (NDA signed, emergency
contacts, legal authorization) described an organization that does not exist. What survives:

- [ ] Scope defined: which repos, which target (AGNOS kernel vs Linux host bootstrap — they are
      **different systems** and the distinction is the single most common source of false results)
- [ ] Target versions recorded from each repo's `VERSION` file, not from documentation
- [ ] Reporting channel confirmed against root [`SECURITY.md`](../../SECURITY.md)
- [ ] Test target is disposable (QEMU image or a re-flashable disk), never the working machine

## Information Gathering

- [ ] Syscall surface enumerated from source, not from docs — dispatch arms in
      `agnos/kernel/core/syscall.cyr` plus `#44 sched_yield` and `#96 fork`, which dispatch from
      `kernel/arch/x86_64/syscall_hw.cyr:101,106` instead and are **not** holes (`#14 pause` is
      answered there too). Cross-check against the authoritative table,
      `agnos/docs/development/agnos-userland-abi.md` (as of 1.57.10: #0–#108, #105 withdrawn)
- [ ] Band map read at `agnos/kernel/core/syscall.cyr:29-58` (⚠️ the comment block is a map, not a
      guarantee — count the `if (num == N)` arms)
- [ ] Listening-port set enumerated **by starting daemons and looking**, not from a rule table —
      AGNOS has no packet filter to consult (see *Network Security*)
- [ ] Kernel binary type confirmed: `readelf -h agnos/build/agnos` — expect `Type: EXEC`
      ⚠️ (see *Kernel Security* → address randomization)
- [ ] Architecture read: [`docs/architecture.md`](../architecture.md) and root `SECURITY.md`,
      which are authoritative over any other doc in this tree

---

## Kernel Security

⚠️ **Renamed from "Kernel Module Security".** There are **no loadable kernel modules** — the AGNOS
kernel is a single Cyrius binary. The `kernel/modules/*/*.c` tree in this repo is Rust-era Linux LKM
source and is not part of AGNOS.

### Memory safety

⚠️ **Cyrius provides none of it.** Everything is `load8`/`load64`/`store64` over raw i64 addresses —
no bounds checking, no pointer types, no ownership, and **no automatic stack protector**. Every bound
in the kernel is a hand-written `if`. This section is therefore the highest-yield part of the whole
checklist, and it must be exercised by hand.

- [ ] Every new syscall handler validates its user pointers with `is_user_range(ptr, len)`
      (`kernel/core/syscall.cyr:513`) — count the call sites yourself
      (`command grep -c 'is_user_range(' kernel/core/syscall.cyr` counts the definition and comments
      too), plus the GPU, VFS and ext2 paths. ⚠️ Since **1.56.52** it is a page-table walk
      (`user_range_mapped`, `:354-378`) requiring present + U/S=1 under the live CR3; **through
      1.56.51 it was a VA-window check that admitted kernel pages** (the kernel image, the kernel-stack
      pool). Gate: `cd agnos && sh scripts/smoke/userwin-smoke.sh` (a `sweep.sh` row).
      ⚠️ `is_user_ptr` (`:509-511`) is only `is_user_range(ptr, 1)` since 1.56.52 and has **no caller
      in `syscall.cyr`** — its one live use kernel-wide is the SHSYS selftest's own ceiling probe
      (`kernel/core/main.cyr:4432`). A new *handler* use of it is still wrong, because it cannot check
      a length
- [ ] Path arguments go through `sc_path_ok(ptr, len)` (`kernel/core/syscall.cyr:594`)
- [ ] Environment blobs go through `sc_env_blob_ok` (`kernel/core/syscall.cyr:609`) — the 16-entry
      cap is load-bearing, not stylistic
- [ ] Integer-overflow / wrap handling: `is_user_range` wrap check present in any new range validator,
      and any `count × stride` length with a ring-3 count goes through `is_user_array`
      (`kernel/core/syscall.cyr:560-565`, 1.56.51) — the wrap check guards an addition, not a product
      the caller already wrapped
- [ ] Use-after-free / double-free: PMM refuses a double free (`kernel/core/pmm.cyr:593-595`)
- [ ] ⚠️ Heap contract audited by hand: `kfree_sized`'s `size` must map to the **same slab class** as
      its `kmalloc` (`kernel/core/heap.cyr:135-144`). A smaller class under-scrubs and leaks the tail
      into a later allocation. **This is an unenforced contract, not a check** — there are no
      redzones, no guard pages, and no double-free detection in the heap
- [ ] Regression coverage actually runs:
      `cd agnos && sh scripts/smoke/syscall-harden-smoke.sh`
      ⛔ This selftest shipped in 1.41.5 and **nothing ran it for ~15 minor versions** — it had even
      stopped compiling. The runner landed 2026-08-05 and is now a `sweep.sh` row (`sweep.sh:614`).
      It is the only coverage epoll / timerfd / signalfd have. A selftest nothing runs is a comment,
      not coverage
- [ ] Ingress clamp non-regression:
      `cd agnos && HARDENING_SELFTEST=1 sh scripts/build.sh && sh scripts/smoke/hardening-smoke.sh`
      (gates on `hardening: ip-clamp PASS`). ⚠️ **This smoke does not self-build** — run it against a
      plain production kernel and it exits at `hardening-smoke.sh:65` with
      *"kernel was not built with HARDENING_SELFTEST=1"*. (The syscall-harden, blk-write and
      blk-ring3 smokes DO self-build, and fault-kill has since 1.57.7; ring3 does not — hence the
      explicit build below)

### Ring-3 / ring-0 boundary

This is the strongest control AGNOS has, and it is genuine.

- [ ] Per-process address space: each proc gets its own PML4/PDPT/PD
      (`agnos/kernel/core/proc.cyr:1419-1568`). Two ring-3 processes cannot see each other's private
      memory through the MMU
- [ ] Kernel pages carry U/S=0: kernel identity PDEs are `0x83`
      (`kernel/arch/x86_64/paging.cyr:7`, `kernel/core/proc.cyr:1483-1485`); user data pages are
      `0x87` (+ NX) and user text `0x85` (`kernel/core/proc.cyr:2366-2445`)
- [ ] SMEP + SMAP enabled when CPUID advertises them — the CR4 block at
      `kernel/arch/x86_64/boot_shim.cyr:308-348`. ⚠️ Silently skipped on a CPU without the bits, and
      ⚠️ **SMAP is open for the whole duration of every syscall** (`stac` before / `clac` after the
      handler call, `kernel/arch/x86_64/syscall_hw.cyr:612-621`), not per-copy as Linux does it
- [ ] W^X on user memory — ⚠️ **verify which half is live on the build you test.** `proc_map_page_nx`
      (`kernel/core/proc.cyr:2431-2445`) stamps bit 63, and the high mmap arena's
      `proc_map_page_hi` does the same (`proc.cyr:2862`). NX today: **the user stack**
      (`kernel/core/elf.cyr:371,649`), **all `sys_mmap` anonymous memory** (both arenas) and **every
      non-`PF_X` segment** (`elf.cyr:322`). Text: since **1.56.51** a `PF_X` segment is mapped
      read-only + executable (`proc_map_page_rx`, `0x85`, `proc.cyr:2415-2429`; `elf.cyr:321`) —
      ⛔ through 1.56.50 text was mapped `0x87`, **writable and executable**. Since **1.57.2** (built
      and gated, not burned) a PT_LOAD asking for PF_W|PF_X is refused, and `ring3-smoke` requires
      `wx: RWX segment refused` (`elf.cyr:281-298`, `:572-580`). ⚠️ This item previously quoted a
      kernel note (`elf.cyr:314-317` at 1.56.40) that cyrius/cyrld emits ONE RWE PT_LOAD per binary;
      1.56.51 measured the toolchain's output as `R-X @0x400000` and `RW-` in a separate 2 MB page,
      and at 1.57.2 `build/rootfs/bin` held 0 RWX binaries — the note is gone
- [ ] ⚠️ **EFER.NXE is inherited from UEFI firmware on the shipping boot path, not set by AGNOS.**
      The gnoboot shim never touches EFER (`kernel/arch/x86_64/boot_shim.cyr:186-356`); only the dead
      ELF32/multiboot1 path does (`:103-105`). On firmware that leaves NXE clear, **every W^X mapping
      becomes a reserved-bit #PF**, and no boot-time assertion checks for it. The comment at
      `proc.cyr:2390` claiming NXE is set at boot is wrong for the shipping path
- [ ] Ring 3 cannot issue port I/O: IOPL 0 (`kernel/arch/x86_64/ring3.cyr:209-214`) and the TSS
      I/O-map base (offset 102) is 104, past the TSS limit of 103, so there is **no I/O permission
      bitmap** (`kernel/arch/x86_64/gdt.cyr:163-187`). Check the boot log for
      `tss: I/O map base at 0x66 OK - no ring-3 port access` (printed on iron at the 1.56.53 burn).
      ⛔ **Through 1.56.51 this box could not be ticked**: the 104 landed in the reserved field at
      offset 100, the real base stayed 0, and ports `0x000–0x33F` — including `0x64` (CPU reset) and
      `0x70`/`0x71` (CMOS) — were open to ring 3. Fixed at 1.56.52
- [ ] GPRs are zeroed before every ring-3 entry so no kernel register content leaks
      (`kernel/arch/x86_64/ring3.cyr:244-285`)
- [ ] SFMASK clears IF|TF|DF|AC on syscall entry (`kernel/arch/x86_64/syscall_hw.cyr:364-376`)
- [ ] Ring-3 fault kills the process, not the machine — the box survives:
      `cd agnos && sh scripts/smoke/fault-kill-smoke.sh` (it builds its own
      `FAULT_SELFTEST=1 EXT2_WRITE_SELFTEST=1` kernel since 1.57.7; gates on `run: exit 142` and
      `fault: SURVIVED back in kernel`).
      ⚠️ All 256 vectors get a default gate (`idt.cyr:40-44`), but only {0 #DE, 6 #UD, 13 #GP, 14 #PF}
      are on the **kill** path (`idt.cyr:547-601`, gated on CPL3 at `:565`). The other installed
      exception vectors (#DF, #TS, #NP, #SS, #XM) **paint the canary and halt the box even from
      ring 3**, an uninstalled error-code vector turns into #DF on the bare-`iretq` default, and any
      CPL0 fault halts (`#NM` is serviced by the lazy-FPU handler, `main.cyr:280`). Vector 0 proved
      that class cost a machine before 1.56.18. ⚠️ And until **1.57.7** a ring-3 **#GP** did not reach
      the kill path: the #GP stub's diagnostics dereferenced the saved user RSP and took a SMAP #PF at
      CPL0 (`idt.cyr:395-407` now skips them for a ring-3 fault; built and gated, not burned)
- [ ] Ring-3 scheduling / preemption holds: `cd agnos && RING3_SELFTEST=1 ./scripts/build.sh && sh scripts/smoke/ring3-smoke.sh`

### Irreversible-operation gates

⚠️ These are **anti-accident gates, not authorization**. The kernel says so itself
(`kernel/core/syscall.cyr:8616-8629`): the arm call is "the exact seam where an aegis
installer-capability check lands" — and since 1.56.56 the same comment names that successor as *"a
KAVACH capability check in userland"*, not a kernel credential (through 1.56.55 it read "when agnos
grows per-proc caps"). Any ring-3 process can arm them today by calling with a constant published in
the source. Test that the gate holds, and understand what holding means.

- [ ] Raw block write refuses until armed, and the armed round-trip is correct:
      `cd agnos && sh scripts/smoke/blk-write-smoke.sh`
      ⛔ **`run: exit 83` or `exit 84` means THE GATE IS BROKEN** — an unarmed raw write succeeded.
      Gate source: `kernel/core/syscall.cyr:8673-8698` (`blk_open_sys`) and `:8764-8765`
      (`blk_write_sys`). The arm is no longer boot-long: `blk_close` clears it since 1.56.51
      (`:8820-8832`), and the arming process's exit clears it since 1.57.1 (foreground reap) / 1.57.7 (any death; built and gated, not burned) (`:8632-8641`;
      the smoke's second run asserts it)
- [ ] Raw block **read** is ungated by design — confirm you understand this before reporting it:
      `kernel/core/syscall.cyr:8704-8758` (the dispatch arm at `:10311-10315` still calls the band
      "non-destructive"). Non-destructive is not non-disclosing; any process can read the whole disk
      (`cd agnos && sh scripts/smoke/blk-ring3-smoke.sh`)
- [ ] Kernel-side modeset sits behind an arm-once on-disk latch — confirm `/.modeset-armed` is absent
      on a system that should not be modesetting (`agnos/kernel/core/modeset_latch.cyr:97`)
- [ ] `shm_free #74` is owner-gated; a non-owner gets −1 (`kernel/core/syscall.cyr:10298-10310`).
      ⚠️ `shm_write #72` / `shm_read #73` are **cross-owner permitted and only counted**
      (`syscall.cyr:10284-10297`). The compositor reads a client's buffer every frame, so this is
      current design, not a defect. ⛔ **The counter is not operator-readable**: `shm_xown_warns`
      (`syscall.cyr:1716`) is a kernel global with no syscall, no boot print and no ring-3 path — its
      only reader is the in-kernel SHSYS selftest (`kernel/core/main.cyr:4513-4515`). Verify this
      behaviour by reading the source, not by querying a running system

### Address randomization

- [ ] ⛔ Confirm for yourself that **no address randomization is active in a shipped AGNOS**:
      `readelf -h agnos/build/agnos` reports `Type: EXEC`. gnoboot only slides an `ET_DYN` image
      (`gnoboot/src/main.cyr:644-671`, gnoboot 0.7.2); an ET_EXEC kernel takes the fixed-load arm at
      `:672-681`
- [ ] ⚠️ The PIE mechanism exists and is testable, but is **opt-in and built by nothing** —
      `CYRIUS_PIE` appears in no CI job, no `scripts/build.sh` default, no `sweep.sh` row and no ISO
      pipeline (re-checked at 1.57.10). To exercise it:
      `cd agnos && CYRIUS_PIE=1 ./scripts/build.sh && sh scripts/smoke/kaslr-smoke.sh`
      (the smoke hard-errors on a non-PIE build — `scripts/smoke/kaslr-smoke.sh:29`)
- [ ] ⛔ The **allocator** half of KASLR is dead code. `pmm_next_free` is RDRAND-seeded
      (`kernel/core/pmm.cyr:535-537`) but **no allocation decision consumes it.** ⚠️ A grep returns
      several hits, not one — expect them: `pmm.cyr:598` and `:719` only *maintain* the hint on free
      (`if (page < pmm_next_free) …`), and the sole consumer of the value is the boot diagnostic
      print at `kernel/core/main.cyr:524-526`. `pmm_alloc` never reads it. The bottom-up first-fit that
      consumed it was removed at 1.41.12 (`pmm.cyr:543-561`). **The boot log still prints a
      `KASLR:` line for a mechanism that does nothing** (and agnos CI's two-boot diff asserts only
      that the printed value varies). Do not read that line as a control

### The S1–S13 set — do not restate the summary

⚠️ `agnos/docs/development/security-hardening.md:236-257` marks S1–S13 "13 of 13 Done", **closed at
v1.28.0** — 29 minor versions before the 1.57.10 this file was last re-verified against (28 before the
1.56.40 it was first written against). (That header line is the only version stamp the section
carries; do not attribute it to any other release.) Several entries have since been silently
invalidated by later code. Re-derive before citing:

- [ ] S7 KASLR — ⛔ **inert, both halves** (above). Do not restate "KASLR data-only"
- [ ] S8 KPTI — ⛔ **collapsed, and it is not a Meltdown mitigation.** `kernel/core/proc.cyr:1526-1545`
      and `kernel/arch/x86_64/syscall_hw.cyr:74-75` both say so: kernel == user == running CR3, the
      CR3 switches are same-value no-ops. Architectural protection (U/S=0) holds; speculative
      side-channel protection does not exist
- [ ] S9 IBRS — ✅ real but **CPUID-gated**: leaf 7 subleaf 0 EDX bit 26
      (`kernel/arch/x86_64/syscall_hw.cyr:403-414`). If the CPU does not advertise it, the MSR writes
      are **not emitted at all** and there is no warning. No retpoline anywhere
- [ ] S10 VT-d IOMMU — ⛔ **not active on the primary iron target.** On AMD the kernel
      **deliberately disables AMD-Vi every boot** — `amd_iommu_disable()` at `kernel/core/main.cyr:636`
      writes the AMD-Vi Control Register to 0, "passthrough for everyone". archaemenid is AMD
      (Cezanne). On the iron AGNOS actually runs on, **DMA is entirely unrestricted**. ⚠️ VT-d itself
      **never enabled translation on any boot through 1.57.9** (the QEMU `q35` + `intel-iommu` boot
      printed `IOMMU: init failed`); since **1.57.10** `iommu_init()` runs every boot
      (`main.cyr:717-736`) and turns translation on when a drivable Intel DMAR unit is present —
      gated by `vtd-smoke` in QEMU only, **not burned**, and a unit with VER ≥ 6 is refused
- [ ] S13 stack canaries — ⚠️ **four hand-placed guards, not `-fstack-protector`.**
      As of 1.57.10 `var canary = stack_canary_secret` appears in exactly four functions:
      `kernel/core/elf.cyr:146` (`elf_load`), `kernel/core/elf.cyr:454` (`elf_load_from_file_body`),
      `kernel/core/syscall.cyr:8871` (`spawn_load_child`, 1.57.7), `kernel/core/syscall.cyr:9417`
      (`ksyscall`). `net_handle_tcp`'s was removed at 1.56.52 (no local buffer to guard,
      `net_tcp.cyr:1389`). The secret is a **global** (`kernel/core/syscall.cyr:4`), not `gs:0x28`,
      so any kernel-memory disclosure reads it
- [ ] S1 / S2 / S6 — re-verified real: U/S separation (`paging.cyr:7`), per-CPU TSS + RSP0
      (`gdt.cyr:116-217`), **user** stack guard pages (`elf.cyr:373,653`).
      ⚠️ S6 is the user-stack row and does not cover kernel stacks. Separately, since **1.57.6** (built and gated, not burned), each 64 KB
      per-process kernel-stack slot keeps its lowest 4 KB not-present (`kernel/core/proc.cyr:662-666`,
      `kernel/core/vmm.cyr:387-426`; `kstack-smoke` checks the guard pages), so an overflow faults to
      `#DF` on IST1. Through 1.57.5 the slots were packed with no unmapped page between them and a
      kernel-stack overflow walked into the neighbour's stack with no fault and no canary

---

## Boot Chain

⛔ **gnoboot performs no cryptographic verification of anything.** Since gnoboot 0.7.0 its
kernel-load checks are structural; through 0.6.x the entire kernel-integrity check was the ELF
magic, twice.

- [ ] Read the whole verification path yourself: `gnoboot/src/main.cyr:482-754` (gnoboot 0.7.2) —
      file size `:507-522`, ELF identity (magic, class, data, `e_type`, `EM_X86_64`) `:524-550`,
      program-header bounds `:557-579`, every PT_LOAD `:581-626`, magic re-checked at the load
      address `:743-754`. There is no hash and no signature
- [ ] Confirm sigil is not in the boot path: `gnoboot/cyrius.cyml` `[deps]` is
      `stdlib = ["fnptr"]` — one stdlib helper, nothing else. Whatever signing capability sigil has,
      **no boot-path code calls it**
- [ ] ⚠️ Confirm the toolchain pin lag: `gnoboot/cyrius.cyml:7` pins `cyrius = "6.6.2"` against the
      6.6.6 toolchain (2026-09-27). When this item was written it pinned 6.2.44 against 6.5.7 — a
      large lag on the one component with no exception handling and nothing between it and the
      firmware. Re-check the gap on every pass
- [ ] Understand the exposure: anyone who can write `\boot\agnos` on the ESP owns the machine, and
      the ESP is a FAT partition
- [ ] Exec-time validation read and understood — `agnos/kernel/core/elf.cyr:145-400` (and its
      from-disk twin at `:447-809`): ELF magic, class=2, `e_entry >= 0x400000`, phdr-table bounds, a
      pre-pass over every PT_LOAD **before** any allocation (`:176-209`, the 1.41.6 fix), and since
      1.57.7 an overlap refusal plus the `spawn_limits` memory cap (`:211-214`). ⛔ **No hash, no
      signature, no sigil reference anywhere in the loader.** Verification happens at **install**, in
      ark — never at exec

---

## Supply Chain & Package Integrity

⛔ **There is no vulnerability-tracking mechanism for AGNOS first-party dependencies. None,
anywhere** — no advisory DB, no CVE feed, no OSV/NVD client, no scanner, no known-bad-version list, in
cyrius/cbt, ark, nous, takumi, zugot or mela. ⚠️ `cyrius audit` is a **quality** sweep
(fmt / lint / docs / tests / bench, per `cyrius --help`), **not** a vulnerability audit. Do not let the
name mislead you. What genuinely exists is integrity and pinning, and it is worth checking:

- [ ] Declared dep tags actually exist upstream:
      `python3 scripts/check-dep-tags.py <repo-dir>` (in this repo). Exit 0 = every declared tag
      exists; network failures are reported as unknown, never as OK
- [ ] ⚠️ `path = "../X"` overrides audited. **A path override disables the tag as a test** — the
      commit-pin check is intentionally bypassed (`cyrius/cbt/deps.cyr:1884-1889`). ark, phylax,
      aegis, bote, mela, kavach, libro and t-ron override **every** dep this way (re-checked
      2026-09-27), so a dev box with sibling checkouts compiles a different program than CI does. CI
      is the first honest build of the declared graph
- [ ] Commit pins hold: `cyrius deps` refuses on HEAD ≠ locked commit (force-pushed/repointed tag),
      a corrupt pin line, or a tampered cache (`cyrius/cbt/deps.cyr:2087-2132`; the cache check is
      `_git_cache_verify`, `:3600-3696`). ⚠️ Tagged deps only — an untagged dep is content-verified
      since cyrius 6.6.5 but never pinned
- [ ] ⚠️ `cyrius deps --verify` run **against a committed lockfile you did not just regenerate**.
      As ordered in every ecosystem CI (`cyrius deps` then `cyrius deps --verify`) the file-hash half
      is tautological — verify compares the lock against the files it was just written from
      (`cyrius/cbt/deps.cyr:185-186`). The commit-pin half is unaffected
- [ ] `cyrius vet <src>` and `cyrius deny <src>` run — include-path policy and traversal/absolute-path
      denial. ⚠️ This is the *entire* audit-tooling surface; `cyrius/programs/cyaudit.cyr` has no
      version awareness and no advisory feed
- [ ] zugot recipe sources are hash-pinned — ⚠️ **not all of them**: recipes carrying
      `sha256 = ""` under the validator's `# TODO` carve-out still declare live remote sources (see
      [vulnerability-management.md](vulnerability-management.md)); the recipes with no `sha256` key
      at all are `local = true` with no upstream source. The parser **fails closed** on a remote
      source whose hash key is absent (`takumi/src/parse.cyr:203-218`); an explicitly empty hash
      passes it and is caught by `takumi validate` and the build-time verify
- [ ] takumi verifies **before** extracting: `fetch_source` → `verify_source_hash` → `extract_archive`
      (`takumi/src/cli.cyr:273-290`), and `verify_source_hash` returns fail on any expected hash that
      is not exactly 64 hex chars (`takumi/src/source.cyr:200-219`)
- [ ] zugot's dated CVE audit cadence is being kept — one immutable report per pass at
      `zugot/docs/audit/YYYY-MM-DD.md` (`zugot/docs/adr/0005-audit-cadence.md`). ⚠️ On 2026-09-27
      the folder still held only `2026-04-17.md` against a monthly cadence. This covers
      **zugot's upstream pins only**. First-party Cyrius code has no advisory process
- [ ] Toolchain releases are signed end-to-end — `cyrius/programs/cyrsign.cyr` (detached Ed25519 over
      `SHA256SUMS`), and `cyrius/.github/workflows/release.yml` refuses to publish unsigned.
      ⚠️ **No other repo in the ecosystem signs its releases**, and a first install is explicit TOFU

### Package install verification (ark)

Every check below is in `ark_pkg_read` (`ark/src/ark_package.cyr:231-389`, ark 1.4.2), called from
`ark_pkg_install_inner` (`:602`) **before any file is written**.

- [ ] `ark verify [package]` runs clean (real subcommand — `ark/src/cli.cyr:144`)
- [ ] SHA-256 root hash over the verified prefix (`ark/src/ark_package.cyr:252-259`)
- [ ] Per-file SHA-256 against the index (`:349-375`).
      ⚠️ Skipped for an entry whose hash field is absent, or not exactly 64 chars
- [ ] Zip-slip guard rejects the **whole package** before writing a single file
      (`ark/src/ark_package.cyr:626-635`)
- [ ] Symlink entries fail closed on a target without symlink support, i.e. agnos (`:636-644`)
- [ ] ⛔ **`require_signed` defaults to 0** — `ark/src/types.cyr:342` (`store64(p + 96, 0)`). Unsigned
      `.ark` files install; signed-by-anyone `.ark` files install. The Ed25519 check at
      `ark_package.cyr:266-272` only runs when the package's own `ARK_FLAG_SIGNED` bit is set, and the
      trust gate at `:607-617` only runs when policy demands it. **Confirm which mode your system is
      in before recording a green here**
- [ ] Trust anchor populated: `/etc/agnos/ark/trusted_keys`
      (`ark/src/types.cyr:24`) — a newline-delimited hex Ed25519 pubkey file.
      ⚠️ A signature without a trust anchor proves nothing about publisher identity
      (`ark/docs/audit/2026-06-18-pre-v1-audit.md:27-28`; the residual is `:91-93`)
- [ ] ⚠️ `require_signed` / `trust_keys` are **not settable from a config file** — no such key is
      parsed by `load_config_from` (`ark/src/cli.cyr`), and there is no flag or environment variable.
      The policy is code-only (re-checked at ark 1.4.2)

---

## Network Security

📋 **AGNOS has no firewall.** No netfilter, no nftables, no packet-filter hook anywhere in the kernel.
Inbound reachability is decided entirely by *what you run*. "Unnecessary services disabled" is not a
check you can perform against a rule table — it is a check you perform against a process list.

- [ ] Listening set enumerated against the daemons you actually started. ⛔ **Do not write "nothing
      else listens"** — that was a false claim already caught once, and no count belongs here either.
      Derive it: `command grep -rln 'INADDR_ANY' /home/macro/Repos/*/src/`. ⚠️ **A hit is not proof
      of a default bind** — on 2026-09-27 daimon, dig, hoosh, kavach and sandhi appeared only in
      comments (several saying they do *not* use it), and agnostic and mirshi bind it only behind an
      explicit env var / flag. Confirm each candidate's actual bind call before listing it
- [ ] ⚠️ **On AGNOS the bind address is not honoured by any server shipped today.** Through cyrius 6.6.6 the stdlib's agnos
      `sock_bind` drops the address (`cyrius/lib/net.cyr:295-302`) and `sock_listen#56` is issued with
      the port alone (`lib/syscalls_x86_64_agnos.cyr:1633-1641`), so every "loopback" listener below
      is on the NIC's address when built for AGNOS. agnos 1.57.7 added a loopback-only listen class
      (built and gated, not burned). cyrius 6.6.7 (2026-09-27) makes the bind address select it
      (`cyrius/lib/net.cyr:300-313` in 6.6.8), but a server carries that only once rebuilt on 6.6.7 or
      later — daimon, hoosh and bote still pin 6.6.6 as of 2026-09-28. Test reachability from a
      second host, not from the box
- [ ] daimon 8090: ⛔ **no authentication whatsoever** — daimon's README and `src/api_agent.cyr:11-12`
      say so, pending its roadmap 2.5.x; port at `daimon/src/config.cyr:11`. Since daimon **2.3.0**
      (its HTTP-listen fix) it binds its `listen_addr`, default `127.0.0.1` (`daimon/src/config.cyr:10`,
      `server.cyr:260-264`; `serve --listen` overrides), and on AGNOS it warns and audits
      `http.listen.not_loopback` when the kernel puts it on the NIC (`server.cyr:1187-1194`). ⛔
      Through daimon 2.2.x both serve modes passed `INADDR_ANY()` and that default was never read. If
      daimon is running, treat its port as exposed wherever it is reachable
- [ ] hoosh 8088: binds `127.0.0.1` by default (`hoosh/src/main.cyr:82,88` — `_http_bind =
      0x0100007F`, and `:85-87` records that this was hardcoded to `INADDR_ANY` before hoosh 2.5.1)
      but ⛔ **auth fails open** — `hoosh/src/lib/auth.cyr:35-36` returns 1 when the token vector is
      empty, and the startup default is an empty vector (`hoosh/src/main.cyr:1041`). ⚠️ The warning
      at `hoosh/src/main.cyr:368-374` fires **only** when bound off 127.0.0.1 *and* no tokens are
      configured, so a loopback deployment is silently unauthenticated. Confirm tokens are configured
      or accept that it is open
- [ ] bote: ⚠️ `bote` with no args is **stdio, not a listener** (`bote/src/main_common.cyr:152` →
      `bote/src/main.cyr:219-220`). A port opens only on an explicit subcommand of the one `bote`
      binary (one binary since bote 3.4.2) — 8390 http (`main.cyr:187`), 8391 bridge (`:203`), 8392
      streamable (`:151`), 8393 ws (`:161`). `bote-streamable` / `bote-ws` are now only argv[0]
      aliases (symlinks, `main_common.cyr:146-149`). All four bind `INADDR_LOOPBACK()`. ⚠️ `bote unix`
      is an AF_UNIX transport — a Linux path only; its agnos arm fails closed
      (`bote/src/transport_unix.cyr:192-200`)
- [ ] bote auth is opt-in via one env var: `BOTE_BEARER_TOKENS=tok-a,tok-b`
      (`bote/src/main_common.cyr:97-114`). ⛔ **Unset ⇒ no validator is installed ⇒ every request
      passes.** When set, the comparison is constant-time (`bote/src/auth.cyr:97-120`)
- [ ] ⚠️ `bote bridge` sets `allowed_origins = ["*"]` — a CORS wildcard (`bote/src/main.cyr:204-206`)
- [ ] agora 2323: binds `INADDR_ANY` (`agora/src/main.cyr:36,3186`), sigil Ed25519
      challenge/response, anon-read / auth-post by default. ⚠️ This is a **telnet** service and it is
      part of the system
- [ ] kavach credential proxy: ⛔ **serves secrets over plain HTTP with no authentication** —
      `GET /v1/secret/<name>` returns the raw secret; the only gates are a per-instance name allowlist
      and a loopback bind (`kavach/src/credential_http.cyr:1-31,97`). On AGNOS, where every process is
      uid 0 with no confinement, **"loopback-only" is not an isolation boundary** — every process on
      the box is a peer
- [ ] Kernel network band understood: the fixed-shape numbers #45–#57, plus `icmp_echo_ex#100` and
      `sock_peer#106`, over one kernel TCP/UDP stack with an 8-slot conn table. ⛔ **Never write
      "AGNOS has no sockets"** —
      `sock_connect#47 / sock_send#48 / sock_recv#49 / sock_close#50 / sock_listen#56 / sock_accept#57`
      exist and work. The true and durable claim is: **no generic BSD socket family, no `socket()`
      over arbitrary domains, no `splice`, no `AF_ALG`** — which is the CVE-2026-31431 structural
      immunity argument, and it survives intact (1.57.7's `sock_listen` address *class* — any vs
      loopback — is not a family selector)
- [ ] Conn ownership — ✅ **since 1.57.7** (built and gated in QEMU, not burned): every TCP and UDP id
      is owned by the incarnation (pid + epoch) that opened it; `sock_send#48` / `sock_recv#49` /
      `sock_close#50` / `sock_accept#57` / `sock_peer#106` and `udp_recv#53` / `udp_unbind#54` return
      −1 to anyone else, and `udp_send#52` refuses a source port another owner bound
      (`agnos/kernel/core/syscall.cyr:10489-10498, 11489-11552, 11569-11641, 11712-11744`). Gate:
      `cd agnos && sh scripts/smoke/sock-owner-smoke.sh` (a `sweep.sh` row). ⛔ Through 1.57.6 there
      was **none** — those arms bounds-checked the id and nothing else, and any process could send
      on, drain, or tear down any connection; a finding against a pre-1.57.7 kernel is documented
      behaviour, one against 1.57.7+ is a regression
- [ ] TLS is a **ring-3** concern (Cyrius stdlib `tls_native` over sigil), not a kernel one. There is
      no TLS in the AGNOS network path

---

## Agent Runtime Security

⛔ **Nothing in the agent path is confined.** kavach's Landlock / seccomp / namespace / cgroup backends
are **Linux-target only**; on AGNOS every one is `#ifdef CYRIUS_TARGET_AGNOS`-gated to fail closed —
`_spawn_enter_rootfs` returns −1 (`kavach/src/confine.cyr:108-118`), `confine_child` (now a wrapper
over `confine_child_wd`) **exits** (`:347-350`), `spawn_namespaces_available` returns 0 (`:431-437`),
`spawn_seccomp_available` returns 0 (`:474-479`), and `confine_capture`'s chain returns −1 in
`_input_env_wd` (`:543-551`). (Lines from kavach's tree on 2026-09-27 — `VERSION` 3.13.1 with
unreleased 3.13.2 work in the source; re-find the five sites by name if they have moved.) kavach's own
comment names the failure mode it is avoiding: *"a sandbox that is silently not a sandbox, which is
the worst failure mode this file has."* What you can check is that it keeps failing **loudly**:

- [ ] kavach reports honestly on AGNOS: every confinement primitive returns a structured
      `err_not_supported` or a negative, **never `Ok(0)`**. A silent success here is a critical
      regression — re-read the five sites above after any kavach bump
- [ ] ⚠️ `backend_sy_agnos` is **inverted from its name** and must not be cited as agnos sandboxing —
      it shells out to `docker` / `podman` to run the agnos image as a container **on a Linux host**
      (`kavach/src/backend_sy_agnos.cyr:1-16`). The Linux host's seccomp/cgroups do the confining.
      agnos has no docker
- [ ] Process isolation that **is** real: per-process CR3 (`agnos/kernel/core/proc.cyr:1419-1568`) and
      per-process fd tables (`agnos/kernel/core/vfs.cyr:148-201`). ⚠️ **The fd-table split can
      degrade to a SHARED table.** `proc_fd_base[pid] == 0` falls back to the global `vfs_table`
      (`vfs.cyr:154-158`), and the source names three triggers (`vfs.cyr:146-147`): pre-init, kthreads,
      **and a kmalloc-failed inherit** — `vfs_fd_inherit` returns at `:197` leaving the child on the
      global table. Through 1.57.6 nothing told the caller, so under memory pressure this was a silent
      isolation loss; since **1.57.7** (built and gated, not burned) `spawn_path#43` and `execwait#37`
      refuse that fallback and tear the child down (−3 / −1, `syscall.cyr:199-212, 245-270`). Other
      creation paths still take the fallback
- [ ] Signal/reap authorization holds: `proc_may_signal` permits init, self, or a direct child only,
      epoch-validated since 1.57.7 (`agnos/kernel/core/proc.cyr:1957-1976`); `proc_may_reap` gates
      `waitpid` (`:1990-1995`). ⚠️ Since 1.57.7 `kill#16` *acts* — 9 ends, 19 stops, 18 continues,
      `0x100` reaches descendants — where through 1.57.6 it only set a pending bit
- [ ] Audio slot and file-lock ownership hold: the slot is stamped with the opener's pid
      (`agnos/kernel/core/syscall.cyr:11891`) and re-checked on every subsequent audio call
      (`:11901, :11913, :11943, :11961, :11973`); `flock #59` holder check (`kernel/core/vfs.cyr:1298-1390`,
      dispatched at `syscall.cyr:11766`)
- [ ] ⚠️ Resource exhaustion: through 1.57.6 `sys_mmap` had **no per-process page budget** and one
      process could `mmap` all machine RAM. Since **1.57.7** (built and gated, not burned) a parent can
      cap its next child's memory and CPU with `spawn_limits#107` — `mmap` returns 0 past the cap
      (`agnos/kernel/core/proc.cyr:3137-3143`) and the loader refuses an image over it. It is opt-in:
      a process nobody capped is still uncapped, and shm, GPU slots, channel rings, pipe buffers, page
      tables and kernel stacks are not counted. `agnos/docs/development/security-hardening.md:269-271`
      still lists the gap as open

## LLM Gateway Security

- [ ] hoosh bind address and auth-token state confirmed (see *Network Security*)
- [ ] ⚠️ Model backend understood: hoosh's default router points at `http://localhost:11434`
      (Ollama) — `hoosh/src/main.cyr:1048`. Plain HTTP to a local process
- [ ] Weight-file provenance: `tula` is the one place a model artifact can be integrity-anchored — a
      typed manifest plus an Ed25519 sigil-signed header. ⚠️ Signing is **optional** (an unsigned
      file is `TULA_SIG_UNSIGNED`) and the readers check structure only — the caller must call
      `tula_verify` (`tula/src/sign.cyr:27`). ⚠️ **Nothing in the agent path is required to use it**,
      and `anukūlana` imports foreign GPT-2 safetensors — and, since 1.1.0, GGUF — with no signature
      requirement
- [ ] ⚠️ Prompt-injection detection: t-ron ships a six-check detector (`t-ron/src/safety.cyr:470-492`)
      over a string with zero-width, bidi-control and BOM characters stripped (`:339-372` — a byte-level
      strip, not NFKC normalization). ⛔ **The MCP path does not call it.** `bote/cyrius.cyml` and
      `daimon/cyrius.cyml` declare no t-ron dep, and no `tron_*` symbol appears in either `src/` tree.
      The dependency runs the other way — t-ron declares `[deps.bote-core]` (renamed from
      `[deps.bote]` in t-ron 2.1.9). ⚠️ One consumer exists outside that path: thoth vendors t-ron and
      runs `tron_check` on its own tool calls when `[tron].policy` is set (`thoth/src/gate.cyr:388-438`)

## MCP Server Security

- [ ] Bearer auth state confirmed per transport (see *Network Security*) — this is the **only** gate
      bote has
- [ ] ⛔ Tool-invocation authorization: **there is none.** t-ron's gate is real code with no caller on
      this path — its deny-code enum (`t-ron/src/gate.cyr:14-21`) names what it *would* refuse:
      `DENY_UNAUTHORIZED` / `DENY_RATE_LIMITED` / `DENY_INJECTION_DETECTED` / `DENY_TOOL_DISABLED` /
      `DENY_ANOMALY_DETECTED` / `DENY_PARAMETER_TOO_LARGE`. Verify by grepping `bote/src/` and
      `daimon/src/` for `tron_` and both manifests for `[deps.tron]`; the correct result today is
      **zero hits in all four**
- [ ] ⚠️ `bote/src/sandbox.cyr` ships an abstract `SandboxRunner` seam and a **noop adapter**. Its
      header names kavach as the canonical choice; bote wires nothing
- [ ] daimon's five libro MCP tools reachable and honest —
      `libro_query` / `libro_verify` / `libro_export` / `libro_proof` / `libro_retention`
      (registered at `daimon/src/mcp_builtin.cyr:25-40`, dispatched at `:67-71`)

## Desktop Environment Security

⛔ **aethersafha is NOT Wayland.** That path is dead; the compositor speaks the sovereign **setu**
protocol with bhumi/mehman backends. ⚠️ `grep -ri wayland aethersafha/src/` does return hits — expect
them and do not read any of them as a Wayland server: `MOZ_ENABLE_WAYLAND` / `GDK_BACKEND` /
`QT_QPA_PLATFORM` env vars set for **guest Linux apps** on the swallow path — browsers at
`apps.cyr:635,639,643,647`, and shruti at `:727-728` — a `PLG_CAP_WAYLAND_SURFACE`
plugin-capability bit (`plugin_host.cyr:69`), and comments (`apps.cyr:715`; the XWayland-parity notes
at `main.cyr:7` and `foreign.cyr:2`; `main.cyr:123`) — lines as of aethersafha 0.16.27. **No Wayland protocol is implemented or served**, so budgeting time for Wayland
protocol fuzzing tests nothing.
⚠️ aethersafha is **pre-1.0** (0.16.27 on 2026-09-27; 0.12.1 when this section was written) and
moving fast; scope it as a moving target, and read the version from its `VERSION` file.

- [ ] setu protocol message handling reviewed (`setu/src/`) — the actual client/compositor seam
- [ ] Input handling and buffer bounds reviewed by hand (see *Memory safety* — the language gives you
      nothing)
- [ ] ⚠️ Window/surface isolation understood as **shm-mediated and cross-owner-permitted**: the
      compositor reads a client's pixel buffer every frame via `shm_read #73`, which is why #72/#73
      are warn-only (`agnos/kernel/core/syscall.cyr:10284-10297`)

---

## Cryptography (sigil)

- [ ] Signing algorithms confirmed from source, not from claims: Ed25519, and **ML-DSA-65 only** for
      post-quantum signing (`sigil/src/mldsa_params.cyr:10` — *"Only ML-DSA-65 (NIST security
      category 3) is parameterised here"*), default-on since 3.7.6 (`sigil/src/lib.cyr:146-168`)
- [ ] ⚠️ "Default-on" means **compiled in, not in use.** On 2026-09-27 ML-DSA was called from
      `libro/src/signing.cyr` (which offers Ed25519 / ML-DSA-65 / hybrid), argonaut's audit-log keygen
      (`argonaut/src/audit_ext.cyr:305-317`) and yantra's hybrid pin-descriptor verify
      (`yantra/src/security.cyr:39-44`); `t-ron/src/main.cyr` mentions it only in comments.
      **ark, takumi, nous, mela, bote, daimon, kavach and the agnos kernel contain no ML-DSA
      reference at all. Every signature in the supply chain today is Ed25519**
- [ ] ⛔ **There is no post-quantum key exchange.** A repo-wide grep for `ml_kem` / `mlkem` /
      `\bkyber\b` over `sigil/src/` returns **zero** (a bare `kyber` also matches "kybernet"), as does
      `sphincs` / `slh_dsa`. `sigil/src/x25519.cyr` is X25519 alone — no hybrid. Do not test for, or
      claim, harvest-now-decrypt-later protection
- [ ] Key rotation surface verified against `sigil/src/` before recording anything about it

## Audit Chain (libro)

- [ ] Chain integrity verifies: `chain_verify` (`libro/src/chain.cyr:349-366`), reachable to an operator
      through daimon's `libro_verify` MCP tool. ⚠️ `libro/src/main.cyr` is a **test harness**, not a
      CLI — there is no `libro verify` command
- [ ] Writers confirmed: daimon — `daimon_audit` (`daimon/src/audit.cyr:39-41`) and
      `daimon_audit_agent` (`:46-48`, `chain_append_with_agent`) are the lifecycle writers;
      `:31` is only the genesis "MCP host initialized" entry, so do not cite it as agent coverage.
      argonaut — `argonaut/src/audit.cyr` + `audit_ext.cyr`, with a chain-integrity verify on
      **replay** (`audit_ext.cyr:125`, rationale at `:121-124`) that refuses a tampered store.
      ⚠️ **ark declares no `[deps.libro]`** despite libro's README naming it a consumer — package
      installs are not on the chain
- [ ] ⚠️ Tamper model stated precisely: the chain makes tampering **detectable, not preventable**.
      `libro/src/file_store.cyr` is append-only JSONL guarded by `flock`; on AGNOS every process is
      uid 0 with no MAC, so any process can truncate or rewrite it. Detection requires an
      out-of-band copy of the head hash
- [ ] ⚠️ Out-of-band anchoring is **opt-in and unwired by default** — `libro/src/anchoring.cyr` and
      `timestamping.cyr` are compiled into the default bundle but nothing on the append or store path
      calls them; `tpm_anchor.cyr` is behind `-D LIBRO_TPM`. Per-entry Ed25519 signing is likewise
      opt-in (`chain_append` never signs)
- [ ] ⛔ **Transparency log: does not exist under any name.** A grep for `transparency` over
      `sigil/src/` and `libro/src/` returns zero. Earlier revisions of this checklist asked you to
      verify its entries

## Scanning & Quarantine (aegis, phylax)

- [ ] ⛔ **aegis is not a daemon.** `aegis/src/main.cyr` sets a log level, optionally opens an
      `AEGIS_EVENT_LOG` NDJSON sink (since aegis 1.1.7), prints `"aegis ready"` and returns. No loop,
      no socket, no hook. Do not record "aegis daemon running and enforcing policies"
- [ ] ⛔ **aegis has zero consumers.** No repo declares `[deps.aegis]` — including daimon and
      argonaut, which its README names. Confirm before assuming anything calls it
- [ ] ⚠️ aegis "scanning" is **metadata-only**: file exists? size 0? world-writable mode bit?
      size > 500 MB? That is the complete list (`aegis/src/lib.cyr:1082-1153` agent — exists /
      size 0 / world-writable; `:1155-1225` package — exists / size 0 / >500 MB; aegis 1.1.8). No
      content inspection, no signature check, no pattern matching. ⚠️ **Do not assume the metadata
      read even works on AGNOS**: `_aegis_stat_modesize` (`:1062-1074`) opens the file `O_NOFOLLOW`
      and calls `sys_fstat`, reading the stdlib's per-target `STAT_MODE` / `STAT_SIZE` offsets — but
      the agnos stdlib's `sys_fstat` is a stub that returns −1 (cyrius 6.6.6
      `lib/syscalls_x86_64_agnos.cyr:953`), so on AGNOS every existing file reports
      `unreadable_metadata`. (An earlier revision of this item said the offsets were Linux's; they
      come from the stdlib per target — agnos's own `stat#33` puts mode at offset 0,
      `agnos/kernel/core/ext2.cyr:295-307`.) aegis has no proven agnos run
- [ ] ⚠️ aegis "auto-quarantine" pushes an entry onto an **in-memory** map when an event is
      CRITICAL/HIGH and config allows (`aegis/src/lib.cyr:469-540`; the map is lazy-initialized at
      `:719-724` and read back via `aegis_quarantine(d)` at `:433`). It returns an advisory code and
      does not kill, isolate or terminate anything, and it does not survive a restart
- [ ] ⚠️ `aegis/src/firewall.cyr` **renders** an nftables ruleset and states in its own header that
      the consumer is responsible for applying it. **Nothing applies it**, and AGNOS has no nftables
- [ ] phylax one-shot scan runs: `phylax scan <path>` (`phylax/src/cli.cyr:1216`) — real YARA engine,
      entropy + chi-squared, PE/ELF parsing, string extraction, SARIF output
- [ ] `phylax rules list` lists loaded rules (`phylax/src/cli.cyr:1299-1305`). ⚠️ The bare verb is
      **not** a command — `phylax rules` with no sub-verb prints
      `Usage: phylax rules <list|validate|fetch>` and exits 1 (`cli.cyr:1292-1295`). Built-ins load
      via `yara_engine_load_builtin_rules` (`phylax/src/yara.cyr:1668`) — **count them yourself**, do
      not restate a number from a doc
- [ ] ⛔ `phylax watch` is unavailable on AGNOS and fails closed with a clear message (no inotify) —
      `phylax/src/cli.cyr:439-448`. `phylax daemon` listens on **AF_UNIX**
      (`phylax/src/cli.cyr:967-1040`), which AGNOS does not have, so it cannot run there at all
- [ ] ⚠️ phylax has **zero consumers** — no `[deps.phylax]` in any manifest. It is a standalone CLI

## Marketplace (Mela) Security

- [ ] Package signature verification on install — real, and it is ark's
      (`ark/src/ark_package.cyr:602-617`). ⚠️ The marketplace path is the one place that **forces**
      `require_signed` on (`ark/src/marketplace.cyr:56-58`), and only the explicit
      `--marketplace <URL>` entry point reaches it — a marketplace step run from a resolved `--apply`
      plan installs through `ark/src/exec.cyr:176-200` under the default. The default path does not
- [ ] ⛔ **Sandbox profiles are generated and then applied by nothing.**
      `mela/src/flutter_agpkg.cyr:233-254` builds a `SandboxProfile` with `landlock_paths` /
      `seccomp_mode` / `network` and emits `sandbox.json`. On AGNOS there is no mechanism that can
      consume it. This is the subtlest trap in the file: an auditor finds real code producing a real
      sandbox descriptor and ticks the box. **The descriptor is inert**
- [ ] Publisher keyring anchoring understood — `ark_trust_from_keyring`
      (`ark/src/ark_package.cyr:433`). ⚠️ At ark 1.4.2 neither it nor `ark_trust_load_keyring`
      (`:455`) has a caller outside the tests; the live trust gate reads the key file (`:611`)

## Data Protection

- [ ] ⚠️ Encryption at rest is **installer-side only**: agnova offers a LUKS root option, but the
      AGNOS kernel **cannot read an encrypted volume**, so it applies to host-staged installs. TPM
      sealing is unbuilt — agnova has no `--tpm` flag
- [ ] Encryption in transit: ring-3 only, via the Cyrius stdlib's `tls_native` over sigil. ⚠️ Not one
      of the ecosystem's listening services listed above serves TLS
- [ ] Audit-log content reviewed for secrets before export (`libro_export`) — the chain is not
      redacted
- [ ] ⚠️ PII / user attribution: audit records **cannot carry a meaningful user identity.** There is
      no uid model; any `"user"` field is supplied by the writer, not established by the system

## Incident Response

⚠️ Root [`SECURITY.md`](../../SECURITY.md) states it flatly: **there is no incident-response tooling
yet.** No dashboard, no alerting, no intrusion-detection service, no forensic-capture command. phylax
provides detection *rules*; libro provides forensic *records*. Neither is an operational IR
capability, and an incident is the worst moment to discover that. What a responder can actually do:

- [ ] Stop the agent through daimon (which owns agent lifecycle). 📋 There is no isolate verb —
      containment today is "stop the process", not "restrict it in place". ⚠️ Stopping means
      something only since agnos **1.57.7** (built and gated, not burned): `kill#16` with 9 ends a
      process, 19 stops it and `0x100` (`KILL_TREE`) reaches its descendants; through 1.57.6 every
      signal only set a pending bit
- [ ] Copy the libro chain files **before anything else**; the chain is hash-linked (per-entry Ed25519 signing is opt-in), so a
      tampered record is detectable — but only against a head hash you already hold
- [ ] Power the machine down cleanly (ACPI S5) or arm nothing further. 📋 There is no lockdown switch
- [ ] ⚠️ Note for the report: **any process can power off or reboot the box** from any pid with a
      published magic pair (`agnos/kernel/core/power.cyr:428-430`). `power_sys` says why in source:
      *"`getuid` is hardcoded 0 and there is no uid model, so a uid check would be a gate that is
      always open"* (`power.cyr:410-411`) — and since 1.56.56 the next lines call that permanent,
      naming a kavach capability gate in userland as the successor (`:412-418`)

## Post-Testing

- [ ] All findings documented with a **live path + line** for each claim
- [ ] Each finding classified against the posture paragraph at the top — documented-behaviour items
      (cross-owner shm, ungated `blk_read`, an uncapped `mmap` by default; unowned conns only on a
      kernel before 1.57.7) are **not** vulnerabilities on AGNOS today and must not be filed as such.
      Record the kernel version you tested
- [ ] Reproduction steps provided as a runnable smoke or a syscall sequence
- [ ] Risk ratings assigned per the CVSS bands below
- [ ] ⚠️ Any doc updated as a result carries the source path it was verified against. A security doc
      describing a control that does not exist is **worse than no document**

---

## ⛔ Not Applicable by Design

These are not unchecked items. AGNOS has no such subsystem, and per
[`docs/development/roadmap.md`](../development/roadmap.md) confinement "when it lands, is
**capability-scoped and native** — never Landlock, seccomp or `unshare` ABI emulation." Testing for
them produces **evidence-shaped emptiness**: an empty result reads as "hardened" when it means "no
mechanism exists." Each line says what replaced it, or that nothing did.

| Removed item | Why it is not applicable | What is there instead |
|---|---|---|
| Landlock enforcement · seccomp filters · syscall allowlist · namespace isolation · cgroup limits | Linux LSM/namespace primitives. kavach implements them for **Linux targets** and fails closed on AGNOS (`kavach/src/confine.cyr:118,348,435-437,477-479,549-551`) | Nothing yet — see *📋 Not Available Yet* |
| uid/gid separation · "root is the only UID 0 account" · SUID audit · `getcap` · `id` / `whoami` | ⛔ **Inverted, not absent.** `getuid` returns 0 for every process (`agnos/kernel/core/syscall.cyr:9667`), and since agnos 1.56.56 that is recorded as the design, not a gap (`planning/identity-and-authorization-model.md`). `find / -perm -4000` finding nothing means "no uid model", not "no SUID risk" | Single-owner posture; sigil keys are identity where identity is needed |
| PAM · password policy · password storage · password reset · brute-force lockout · session tokens/timeout/fixation | No accounts, no login, no sessions. `aegis/src/pam.cyr` and shakti's PAM path are **Linux-host-only** | agora has real per-user sigil Ed25519 challenge/response (`agora/src/main.cyr`), and the descent MUD a per-character passphrase login (`cyrius-yeomans-descent/src/persist.cyr:2395-2404`) — scope any auth test **to the service by name** |
| Horizontal / vertical privilege escalation · permission escalation · permission revocation | There are no privilege levels to move between and no permission system to revoke from. ⚠️ shakti is pre-1.0 (0.7.2), authenticates against a userland policy file, and **has no kernel seam behind it** — since agnos 1.56.56, applying the 2026-05-12 identity-model ruling, the kernel records it as N/A on agnos by architecture | The syscall-boundary arm gates (block-write, modeset latch) — anti-accident, not authorization |
| chroot · mount options · `/etc/fstab` · sysctl hardening · `/proc/self/{ns,cgroup,status}` | None of these exist. `mount#11` and `umount#24` are no-op stubs (`agnos/kernel/core/syscall.cyr:9670,9668`) | — |
| systemd · auditd · rsyslog · `systemctl` | PID 1 is **kybernet** (with argonaut as its library) | sakshi for logging; libro for the audit chain |
| `cargo audit` · `cargo-deny` · `cargo outdated` · `Cargo.toml` pinning · RustSec advisory DB · crates.io ecosystem rules | ⛔ **The toolchain is not Rust.** No cargo, no crates.io, no `Cargo.toml` anywhere in the repo. The monolith was dismantled 2026-04-01 | The real chain: `cyrius.cyml` + `cyrius deps` → zugot recipes → nous → takumi → ark. See *Supply Chain & Package Integrity* |
| Loadable kernel module security | The AGNOS kernel is a **single Cyrius binary** — no module loader, no eBPF | *Kernel Security* above |
| mTLS · client certs · local CA · cert rotation | No mTLS implementation exists in sigil, daimon, hoosh or bote. `sigil/src/certpin.cyr` provides SPKI pinning primitives, not an mTLS stack | — |
| ML-KEM / Kyber key exchange · SPHINCS+ · PQC algorithm negotiation · downgrade-attack prevention | ⛔ Zero occurrences in `sigil/src/`. There is no handshake to negotiate or downgrade | ML-DSA-65 **signing** only (`sigil/src/mldsa_params.cyr:10`) |
| Wayland protocol security | aethersafha speaks **setu**, not Wayland | setu protocol review, above |
| Firewall rules · network segmentation · TCP wrappers · protocol-family disable (DCCP/SCTP/RDS/TIPC) | No packet filter, and **no protocol-family selector at all** — which is the structural immunity, not a config gap | Enumerate the daemons you started |
| `ptrace` · `LD_PRELOAD` / dynamic linker · io_uring · `AF_UNIX` · inotify · docker | Not implemented | — |
| `fork` races / copy-on-write abuse | ⚠️ **Moved out of this table.** `fork#96` shipped at agnos 1.56.54 (corrected 1.56.55) — a full address-space copy, **not** copy-on-write, so there is no CoW fault path to attack; the fork itself is now real surface, with no AGNOS consumer yet (agora's fork-per-accept is its Linux arm; on agnos it always serves by poll) | Test it as surface: `cd agnos && sh scripts/smoke/fork-smoke.sh` |

## 📋 Not Available Yet

Intended, designed, **not built**. Nothing below is a tickable box; each is a roadmap item with the
evidence that it is unbuilt.

- 📋 **Per-process / per-agent capabilities.** Not in the kernel, not in daimon, not behind any config
  schema. `grep -i capabilit daimon/src/` returns no capability *enforcement* (on 2026-09-27 its only
  hits were edge-node capability reporting and a comment); `AgentHandle`
  (`daimon/src/agent.cyr:45-88`) has no capability field. The kernel names the seam as future work
  (`agnos/kernel/core/syscall.cyr:8616-8629`, `power.cyr:410-418`, `proc.cyr:1957-1966`) — and since
  1.56.56 the first two say the successor is a **kavach capability gate in userland**, not a
  per-process kernel credential
- ⚠️ **Per-child resource caps — no longer absent.** Since agnos 1.57.7 (built and gated, not
  burned) `spawn_limits#107` caps a child's memory pages and CPU time; opt-in, set by the parent, and
  blind to shm, GPU slots, channel rings, pipe buffers, page tables and kernel stacks (*Agent Runtime
  Security* above)
- 📋 **Native confinement.** Capability-scoped and sovereign when it lands — explicitly not a Linux
  ABI emulation
- 📋 **Signature verification at exec.** The ELF loader checks magic, class, bounds, segment
  overlap, W^X and the memory cap, and nothing that identifies the binary
  (`agnos/kernel/core/elf.cyr:145-400`). Verification is an **install-time** property of ark
- 📋 **Secure Boot / measured boot.** Post-v1.0 in `gnoboot/docs/development/roadmap.md:366-380,406-410`.
  The pieces that exist are gnoboot itself and `cyrius sign-efi`; **no enrolment flow chains them**
- 📋 **Human-approval / kill-switch surface.** Earlier revisions of the security guide showed
  `security.request_permission(...)` and `security.emergency_kill_switch()`. **That `security.*`
  namespace does not exist in any language.** ⚠️ **But do not record a clean grep here** — a
  near-namesake pair ships in Cyrius today: `sec_ui_request_permission`
  (`aethersafha/src/security_ui.cyr:515`) and `sec_ui_emergency_kill_switch` (`:648`). ⛔ **Both are
  inert UI state.** The "kill switch" sets an `emergency` flag, sets `human_override`, un-approves
  pending override entries in a vec and prints a line — it kills no process and revokes no grant
  (there are no grants: no capability model exists). Both have **zero callers outside
  `security_ui.cyr`**. This is the same trap as mela's sandbox profiles: real code, real name,
  nothing behind it
- 📋 **Agent injection defense (L1–L6).** `docs/development/planning/agent-injection-defense.md` is a
  good design spine and is **entirely un-actioned** — cite it as a roadmap, never as a control
- 📋 **Vulnerability tracking for first-party code.** zugot's dated CVE cadence covers upstream pins
  only
- 📋 **Incident-response tooling.** No IDS service, no alerting, no dashboard, no forensic capture
- 📋 **Meltdown mitigation.** KPTI is collapsed; there is no retpoline
- 📋 **A shipped PIE kernel.** The mechanism exists and was boot-validated; no build path invokes it
- 📋 **SMP re-judgement.** `agnos/docs/development/security-hardening.md:276-277`: *"**SMP**
  invalidates invariant (1). Every finding in this document judged 'not a race' must be re-judged when
  APs are scheduled."* APs are being scheduled. The two SMP-hole issues this item cited (dated
  2026-08-02 and 2026-08-03) are **resolved and archived** under `agnos/docs/development/issues/archived/`
  — the APs never set `EFER.NXE` (fixed 2026-08-03), and the unlocked low-arena `mmap` cursor (fixed
  at 1.56.36, with a second defect in the same function closed at 1.56.53). Both were already fixed
  when this item was written; their status headers did not say so until a 2026-08-07 sweep. The
  re-judgement itself is still owed

---

## Vulnerability Severity Criteria

### Critical (CVSS 9.0-10.0)
- Remote code execution
- Complete system compromise
- Data exfiltration

### High (CVSS 7.0-8.9)
- Privilege escalation
- Sensitive data access
- Service disruption

### Medium (CVSS 4.0-6.9)
- Limited privilege escalation
- Limited data access
- Temporary service impact

### Low (CVSS 0.1-3.9)
- Information disclosure
- Minor configuration issue
- Minimal impact

⚠️ **Calibrate against the posture, not against Linux.** "Privilege escalation" presumes privilege
levels; AGNOS has one. A finding that a process reached uid 0 is not a finding — it started there. Rate
against what an attacker *gains*: reaching ring 0 from ring 3, escaping the per-process address space,
or persisting across a reboot are the real Critical/High shapes on this system.
