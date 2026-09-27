# Security Guide

**Last Updated**: 2026-09-27 (re-verified against agnos 1.57.10 · cyrius 6.6.6 · gnoboot 0.7.2; rewritten for the Cyrius/agnos era 2026-08-05 against agnos 1.56.40 · cyrius 6.5.7 · gnoboot 0.6.1). ⚠️ agnos **1.57.2 → 1.57.10 are built and gated in QEMU but not burned** — the last iron burn is 1.57.1 (2026-09-08). Every control below that landed in that span says so.

This guide documents the security architecture of AGNOS **as it exists in source today**, not as it is intended to exist. It replaces a 2026-03 revision that described a Landlock/seccomp/namespace sandbox model, a per-agent permission system, a human-approval flow, an emergency kill switch, mutual TLS and post-quantum key exchange — **none of which were ever built**, and several of which AGNOS has permanently ruled out.

> **The one rule this document is written under**: a security document that describes a control which does not exist is worse than no document, because it tells an operator they are protected when they are not. Every ✅ item below was read in source at the cited `path:line`. Where a control is absent, this page says so loudly rather than omitting it.

### Status markers

| Marker | Meaning |
|---|---|
| ✅ | **Shipped and verified** — read in source at the cited path:line |
| 🐧 | **Linux-target only** — the code exists but compiles out on AGNOS |
| 📋 | **Intended** — designed or on the roadmap; not a mechanism that runs |
| ⛔ | **Absent** — does not exist in any form. Named here so nobody assumes it |

> **Notable structural-immunity entry**: the AGNOS-native kernel is **structurally immune** to **CVE-2026-31431** (Linux LPE in `algif_aead`/AF_ALG via `splice()`). The sovereign syscall table has **no generic BSD socket family, no `socket()` over arbitrary domains, no `splice`, no AF_ALG** — the bug class is unreachable. ⚠️ Never phrase this as "AGNOS has no sockets": AGNOS ships a curated network band (`sock_connect`#47 / `sock_send`#48 / `sock_recv`#49 / `sock_close`#50 / `sock_listen`#56 / `sock_accept`#57 / `sock_peer`#106, handlers at `agnos/kernel/core/syscall.cyr:10489-10498` and `:11465-11744` as of 1.57.10). Those take a slot-indexed `conn_id` and **no address-family argument**, so there is no family to select. (Since 1.57.7 `sock_listen`#56 carries an address *class* — any vs loopback-only — which is not a protocol-family selector.) The immunity is anchored on the *shape* of the surface, not its size — the table has grown repeatedly and the property holds. This is the canonical **absence-by-design** pattern; see [SECURITY.md](../../SECURITY.md).

## Security Architecture

### Defense in Depth

AGNOS layers security **natively**. ⚠️ No layer below is a Linux mechanism, and none will be: per [`docs/development/roadmap.md`](../development/roadmap.md), confinement "when it lands, is **capability-scoped and native** — never Landlock, seccomp or `unshare` ABI emulation." Layers that are not built are marked, not dressed in a borrowed name.

```
┌──────────────────────────────────────────────────────────────┐
│ Layer 6: Agent / Application                                 │
│   ⛔ per-agent capabilities · ⛔ human-approval gate           │
│   📋 t-ron MCP gate (built; not wired into bote or daimon)    │
├──────────────────────────────────────────────────────────────┤
│ Layer 5: Service                                             │
│   ⚠️  daimon 8090: no auth; on AGNOS listens on the NIC       │
│   ⚠️  hoosh 8088 auth fails OPEN when no tokens configured    │
├──────────────────────────────────────────────────────────────┤
│ Layer 4: Install-time trust                                  │
│   ✅ ark: SHA-256 root hash + per-file hashes                 │
│   ⚠️  Ed25519 signature only when the package carries one;    │
│       require_signed defaults to 0                           │
├──────────────────────────────────────────────────────────────┤
│ Layer 3: Process isolation                                   │
│   ✅ per-process CR3 · ✅ ring-3 separation · ✅ W^X/NX        │
│   ⛔ no uid · ⛔ no capabilities · ⛔ no confinement            │
├──────────────────────────────────────────────────────────────┤
│ Layer 2: Kernel ingress                                      │
│   ✅ user-range validation on every pointer argument          │
│   ✅ SFMASK entry sanitization · ✅ SMEP/SMAP · ✅ IOPL 0      │
│   ⚠️  no LSM framework and no loadable modules — by design    │
├──────────────────────────────────────────────────────────────┤
│ Layer 1: Boot / hardware                                     │
│   ⛔ gnoboot checks ELF structure only: no hash or signature  │
│   ⛔ no Secure Boot enrolment · ⛔ no TPM measured boot        │
│   ⚠️  AMD-Vi IOMMU deliberately DISABLED on the iron target   │
└──────────────────────────────────────────────────────────────┘
```

### The one-sentence honest model

> **AGNOS runs one owner's trusted software on one machine.** Its security value is a radically small, sovereign attack surface that structurally lacks whole classes of Linux vulnerability — **not** the enforcement of boundaries between processes, users, or code of differing trust, none of which the kernel implements today.

That is a coherent posture for a single-owner pre-Beta founder-dogfood system. It is **not** a multi-user posture and must never be described as one.

## Threat Model

### Assets

1. **User Data** — files, keys, personal information
2. **System Integrity** — the kernel image, the ESP, the installed package set
3. **Agent Operations** — AI agent tasks and decisions
4. **Communication** — network traffic, IPC, shared-memory surfaces
5. **Resources** — CPU, GPU, memory, storage

### Threat Actors

1. **Malicious or compromised agents** — the primary concern for an agent-hosting OS
2. **External attackers** — network-reachable services (see the listening-services table below)
3. **Supply chain** — a compromised dependency, recipe source, or `.ark` package
4. **Physical / firmware** — anyone who can write the ESP owns the machine (see §12)

⚠️ **"Insider threats — malicious users or processes" is not a meaningful actor on AGNOS today.** There are no users, and every process is already fully privileged. It returns as an actor the day a uid or capability model lands.

### Attack Vectors

⚠️ This table previously listed "Sandboxing, seccomp, namespaces" and "no root for agents" as mitigations. **A mitigation column that names a control which does not exist is worse than an empty one.** It now says what actually stands in the way — often nothing.

| Vector | Risk | What actually stands in the way today |
|--------|------|----------------------------------------|
| Agent escapes its intended scope | **High** | ⛔ **Nothing.** There is no sandbox, no capability set, and no uid. A process is limited only by the syscalls that exist |
| Privilege escalation | **N/A — nothing to escalate to** | ⛔ Every process is already uid 0 (`getuid` returns 0 unconditionally, `agnos/kernel/core/syscall.cyr:9667`) |
| Kernel compromise via a bad user pointer | **High** | ✅ `is_user_range()` validates every pointer argument — since **1.56.52** a page-table walk that requires present + U/S=1 under the live CR3 (`syscall.cyr:354-378, 513-533`); ✅ `is_user_array` (1.56.51) guards `count × stride` lengths; ✅ SFMASK entry sanitization; ✅ SMEP/SMAP. ⛔ **Through 1.56.51 the check was a VA window only** and admitted kernel pages inside it — measured at 1.56.52: `write(1, 0x300000, 4096)` disclosed the kernel image and a `read()` onto a syscall kernel stack gave ring-0 execution (§3). ⚠️ The kernel is memory-unsafe by construction and hand-audited |
| Cross-process data theft (private memory) | Low | ✅ Per-process CR3 — the strongest control AGNOS has |
| Cross-process data theft (shared memory) | **High** | ⛔ `shm_write`#72 / `shm_read`#73 are **cross-owner permitted**, warn-counted only (`syscall.cyr:10284-10297`). Only `shm_free`#74 is owner-gated (`:10298-10310`) |
| Connection hijack / teardown | Low since 1.57.7 (**High** through 1.57.6) | ✅ Since **1.57.7** (built and gated in QEMU, **not burned**) every TCP and UDP id is owned by the process incarnation (pid + epoch) that opened it: `sock_send`#48 / `sock_recv`#49 / `sock_close`#50 / `sock_accept`#57 / `sock_peer`#106 and `udp_recv`#53 / `udp_unbind`#54 return −1 to anyone else, and `udp_send`#52 refuses a source port another owner bound (`syscall.cyr:10489-10498, 11489-11552, 11569-11641, 11712-11744`). ⛔ Through 1.57.6 the 8-slot conn table had **no owner field** and those arms bounds-checked the id and nothing else |
| Disk disclosure / destruction | **High / Medium** | ⛔ `blk_read`#77 is ungated by design. ⚠️ `blk_write`#78 requires an arm with a constant published in the source — anti-accident, **not** anti-adversary. The arm no longer lasts the boot: `blk_close` clears it since 1.56.51, and the arming process's death clears it since 1.57.1 / 1.57.7 (§6) |
| Memory-exhaustion DoS | **High** by default | ⚠️ Since **1.57.7** (built and gated, **not burned**) a parent can cap its next child's memory and CPU with `spawn_limits`#107 — `mmap` returns 0 past the cap (`proc.cyr:3137-3143`). It is opt-in, a process nobody capped is uncapped, and it does not count shm, GPU slots, channel rings, pipe buffers, page tables or kernel stacks. ⛔ Through 1.57.6 `sys_mmap` had **no per-process page budget** at all; one process could take all machine RAM |
| Shellcode on heap or stack | Low | ✅ W^X — every non-executable segment, every user stack and all `mmap` memory is mapped NX, and since 1.56.51 text is mapped read-only (§2) |
| DMA from a malicious PCI device | **High on AMD iron** | ⚠️ AMD-Vi is **deliberately disabled every boot** (`main.cyr:636`). VT-d translation **never turned on through 1.57.9**; at **1.57.10** it turns on for the first time when a drivable Intel DMAR unit is present — gated in QEMU `q35` + `intel-iommu`, **not on iron**, and the iron target is AMD |
| Tampering with the kernel image | **High** | ⛔ **Nothing verifies it, ever** — not at boot, not at exec. The ESP is a FAT partition |
| Prompt injection into an agent | **High** | 📋 t-ron ships a six-pattern injection detector (`t-ron/src/safety.cyr:470-492`) that **the MCP path does not call** — neither bote nor daimon declares a t-ron dep. (thoth, the agentic coding TUI, vendors t-ron and gates its own tool calls through `tron_check` when a policy file is configured — `thoth/src/gate.cyr:388-438`) |
| Supply-chain compromise of a dependency | Medium | ⚠️ TOFU commit-pinning against `cyrius.lock`, **tagged deps only** (`cyrius/cbt/deps.cyr:2087-2132` in cyrius 6.6.6): a `git` dep with no `tag` is not pinned and floats on its branch head. Since cyrius 6.6.5 its cache is at least content-verified against the remote (`:2054-2057`, `:2080-2086`); through 6.6.4 the whole check sat behind `if (dep_tag != 0)` and an untagged dep skipped verification entirely. ✅ `.ark` hash + optional signature at install. ⛔ **No advisory database of any kind** — see [vulnerability-management.md](vulnerability-management.md) |

## Security Features

### 1. Process isolation — ✅ shipped

The strongest control AGNOS has, and it is genuine.

- **Per-process address space.** `proc_create_address_space` builds a fresh PML4/PDPT/PD per process (`agnos/kernel/core/proc.cyr:1419`). Two ring-3 processes cannot see each other's private memory through the MMU.
- **Ring-3 separation.** CS=0x23 / SS=0x1B at DPL 3 (`agnos/kernel/arch/x86_64/ring3.cyr:208,216`). Kernel identity PDEs carry `0x83` — present + writable + 2 MB, **U/S clear** (`proc.cyr:1483-1485`); user pages carry U/S set — `0x87` data (plus NX), `0x85` read-only text (`proc.cyr:2366-2445`). Ring 3 cannot reach a kernel page.
- **SMEP + SMAP**, CPUID-gated, enabled on the shipping gnoboot boot path (`agnos/kernel/arch/x86_64/boot_shim.cyr:308-348`). Ring 0 cannot execute a user page.
- **No port I/O from ring 3 — since 1.56.52.** The TSS I/O-map base (offset 102) is 104, past the TSS limit of 103, so there is **no I/O permission bitmap** (`agnos/kernel/arch/x86_64/gdt.cyr:163-187`), and ring 3 runs at IOPL 0 (`ring3.cyr:214`). Any `in`/`out` takes #GP. ⛔ **Through 1.56.51 this did not hold**: the 104 was written to offset 100 (a reserved field), the real base stayed 0, and the CPU read a bitmap from the zero-filled TSS itself — ports `0x000–0x33F` were open to ring 3, including `0x64` (CPU reset), `0x70`/`0x71` (CMOS, NMI mask), the 8259 masks and the PIT. A boot gate now asserts the placement; it printed `tss: I/O map base at 0x66 OK - no ring-3 port access` on iron at the 1.56.53 validation burn (2026-08-30).
- **Ring-3 faults kill the process, not the box**, for vectors {0 #DE, 6 #UD, 13 #GP, 14 #PF} (`agnos/kernel/arch/x86_64/idt.cyr:565`). ⚠️ The #GP half was found broken and fixed at **1.57.7** (built and gated, not burned): the #GP stub's diagnostics dereferenced the faulting context's saved RSP — a user address for a ring-3 #GP — so a privileged instruction, a DPL0 `int` or a bad segment load from ring 3 took a SMAP #PF at CPL0 and halted the box; a CPL guard now skips those reads (`idt.cyr:395-407`).
- **GPR zeroing before every ring-3 entry** so no kernel register content leaks to a fresh program (`ring3.cyr:244-285`).

⚠️ **The per-process CR3 is a deliberate superset of the kernel map.** All kernel entries are mirrored into it as U/S=0. Ring 3 still cannot touch them, but a *kernel* dereference of a bad user pointer reads real kernel or MMIO memory — it does not fault harmlessly. That is why the validators in §3 are load-bearing.

⚠️ **KPTI is collapsed.** Since 1.40.3 the "user" CR3 stashed per process **is** the full kernel CR3 (`proc.cyr:1526-1545`; `syscall_hw.cyr:74-75` states the CR3 installs are "same-value no-ops today"). Architectural protection holds; **Meltdown side-channel protection does not exist.** There is no retpoline.

### 2. W^X / NX — ✅ shipped for user pages, ⛔ absent for the kernel

`proc_map_page_nx` stamps bit 63 (`agnos/kernel/core/proc.cyr:2431-2445`) and is applied to every non-`PF_X` PT_LOAD segment, the user stack (`agnos/kernel/core/elf.cyr:371`), and all anonymous `mmap` memory. Shellcode written to a heap or stack is not jumpable. Executable segments are mapped **read-only + executable** (`proc_map_page_rx`, flag `0x85`, `proc.cyr:2415-2429`) since **1.56.51** — ⛔ through 1.56.50 every ELF text page was mapped `0x87`, writable *and* executable, so only the NX half of W^X existed. Since **1.57.2** (built and gated, not burned) a PT_LOAD asking for both W and X is refused outright, in both loaders (`elf.cyr:281-298`, `:572-580`).

- ✅ **User stack guard pages.** Both ELF loaders unmap the 2 MB below the stack to trap overflow (`elf.cyr:372-373`, `:653`).
- ⚠️ **EFER.NXE is inherited from firmware on the shipping boot path.** The gnoboot shim never writes EFER (`boot_shim.cyr:186-356`); an in-source comment elsewhere claiming "EFER.NXE is set at boot" (`proc.cyr:2390`) is true only of a dead legacy path. On firmware that leaves NXE clear, an NX mapping becomes a reserved-bit #PF. Nothing asserts this at boot.
- ⛔ **Kernel-side W^X does not exist.** `pt_init` identity-maps all 512 PD entries as present + writable with no NX (`agnos/kernel/arch/x86_64/paging.cyr:5-27`) — **kernel `.text` is RWX**. This is architectural: the syscall entry stub, the IDT stubs and the AP trampoline are all runtime-emitted machine code written into writable pages.
- ⚠️ **Kernel stack guard pages — since 1.57.6, built and gated in QEMU, not burned.** Each process's 64 KB kernel stack slot now keeps its lowest 4 KB not-present (`proc.cyr:662-666`, `vmm.cyr:387-426`), so an overflow faults into `#DF` on its own IST1 stack instead of writing into the neighbour; `BOOTCR3_KEEP_GNOBOOT_CR3` builds get no guard pages. ⛔ Through 1.57.5 the slots were packed contiguously with no unmapped page between them, and a kernel-stack overflow walked silently into the neighbour's stack.

### 3. The syscall trust boundary — ✅ shipped

Every pointer a ring-3 caller passes is validated before the kernel touches it. Quoted from `agnos/kernel/core/syscall.cyr:509-533, 594-597` as of 1.57.10, with the in-source comments elided — this is kernel source, not an API to call:

```cyrius
fn is_user_ptr(ptr) {
    return is_user_range(ptr, 1);
}

fn is_user_range(ptr, len) {
    if (len < 0) { return 0; }
    if (ptr + len < ptr) { return 0; }
    if (ptr < 0x200000) { return 0; }
    if (ptr + len > 0x40000000) {
        if (ptr < himmap_floor()) { return 0; }
        if (ptr + len > HIMMAP_CEILING) { return 0; }
    }
    var cr3 = user_access_cr3();
    if (cr3 == 0x1000) { return 1; }
    return user_range_mapped(cr3, ptr, len);
}

fn sc_path_ok(ptr, len) {
    if (len < 1) { return 0; }
    return is_user_range(ptr, len);
}
```

`user_range_mapped` (`:354-378`) walks PML4 → PDPT → PD under the **live** CR3 and requires every 2 MB page in the range to be present **and U/S=1** — the window is only a cheap pre-filter. The `0x1000` branch is the boot-CR3 exemption for in-kernel self-tests; ring 3 cannot reach it, because the SYSCALL stub runs on the calling process's CR3. `is_user_array(ptr, count, stride)` (`:560-565`, 1.56.51) bounds a ring-3 `count` by division before multiplying, because the wrap check guards an addition and cannot guard a product the caller already wrapped. `is_user_range` is called from every buffer-taking arm in `syscall.cyr`, plus the GPU, VFS and ext2 paths. An unknown syscall number returns −1.

⛔ **Through 1.56.51 these were pure VA-window checks** — `is_user_range` returned 1 for any `[0x200000, 0x40000000)` range. Under every per-process CR3 that window also holds the kernel's supervisor identity pages, and a syscall runs with AC=1, so any arm doing raw loads and stores on a "validated" pointer read or wrote kernel memory on ring 3's behalf: measured at 1.56.52, `write(1, 0x300000, 4096)` disclosed 4 KB of the kernel image and `read(fd, 0xF08000, 0x8000)` wrote over a syscall kernel stack (`syscall.cyr:318-353` records it). Closed at 1.56.52; the `userwin` smoke gates both directions. The page walk is also what let the high arena in: the window now admits `[himmap_floor(), HIMMAP_CEILING)`.

⚠️ **There is no copy-in.** Handlers read and write the user VA directly under the caller's CR3, inside a SMAP window the entry stub opens once — `stac` before the handler, `clac` after (`agnos/kernel/arch/x86_64/syscall_hw.cyr:617,621`). **SMAP is disabled for the entire duration of every syscall**, not per-copy as Linux does it. (Since 1.57.7 — built and gated, not burned — every exception stub begins with `clac`, so a fault taken inside a syscall no longer runs its handler with SMAP off — `idt.cyr:169-176`.)

⚠️ **A documented invariant is stale.** `agnos/docs/development/security-hardening.md:17` (invariant 3) still asserts every legitimate user address is below 1 GB. A HIGH `mmap` arena at `[128 GB, 512 GB)` has existed since 1.50.2, and through 1.56.51 every validator rejected it — failing *closed*. Since 1.56.52 `is_user_range` admits it, and only `is_user_array` still bounds its count against the low window (a conservative refusal, recorded in the 1.56.52 CHANGELOG). Do not cite the invariant as written.

### 4. Syscall entry hardening — ✅ shipped, with one CPUID gate

- ✅ **SFMASK = 0x40700** clears IF | TF | DF | AC on SYSCALL entry (`syscall_hw.cyr:364-376`). Each bit closes a named attack: TF single-steps the kernel into the bare-`iretq` #DB gate, DF reverses `rep`-string ops, AC pre-disables SMAP ahead of the stub's STAC.
- ⚠️ **IBRS is CPUID-gated** (leaf 7, subleaf 0, EDX bit 26 — `syscall_hw.cyr:403-414`). If the CPU does not advertise it, the MSR writes **are not emitted at all**, and there is no mitigation and no warning.
- ✅ **Stack canaries** — but be precise about what they are. `stack_canary_secret` is RDRAND-seeded at boot with a timer-mixer fallback (`syscall.cyr:6`), and `stack_canary_check` panics on mismatch (`syscall.cyr:20`). ⚠️ **Cyrius emits no automatic stack protector.** As of 1.57.10 exactly **four** functions in the whole kernel snapshot the canary: `elf_load` (`elf.cyr:146`), `elf_load_from_file_body` (`elf.cyr:454`), `spawn_load_child` (`syscall.cyr:8871`, since 1.57.7) and `ksyscall` (`syscall.cyr:9417`). `net_handle_tcp`'s guard was removed at 1.56.52 — that function has no local buffer for it to protect (`net_tcp.cyr:1389`). It is a global (`syscall.cyr:4`), not `gs:0x28`, so it is readable by any kernel-memory disclosure.

### 5. Address-space randomization — 📋 built, ⚠️ not in a shipped build

⚠️ **No address randomization is active in a shipped AGNOS today.** Both halves need naming:

- ⛔ **The allocator half is dead.** `pmm_next_free` is RDRAND-seeded (`agnos/kernel/core/pmm.cyr:535-537`) but **no allocator reads it** — `pmm_alloc` scans top-down (`pmm.cyr:562`), the 2 MB allocators scan from fixed ends. The comment at `pmm.cyr:543-561` records why: the bottom-up first-fit that consumed the seed scattered a 4 KB page into a random 2 MB region and caused a boot-flaky ring-3 #PF. Removed at 1.41.12. Its only surviving reader is a boot diagnostic print (`main.cyr:524-526`).
- 📋 **The image-base slide requires an opt-in build flag nothing sets.** `agnos/scripts/smoke/kaslr-smoke.sh:7,26-29` requires `CYRIUS_PIE=1 ./scripts/build.sh` and hard-errors on the default build: *"build/agnos is ET_EXEC (non-PIE) — KASLR needs: CYRIUS_PIE=1"*. `CYRIUS_PIE` appears in no CI job, no `build.sh` default, no `sweep.sh` row and no ISO pipeline. A `readelf -h` of the 1.57.10 kernel reads `Type: EXEC`. gnoboot only slides `e_type == 3`; an ET_EXEC kernel loads at its fixed `p_paddr`.

**Consequence, stated plainly**: kernel text, every global, and every gadget sit at a compile-time-known address in the kernel that ships.

### 6. Irreversible-operation arming — ✅ shipped, and honest about itself

AGNOS gates irreversible operations at the syscall boundary rather than behind a MAC label store. Raw block writes are off by default and enabled only by `blk_open(_, BLK_RW_ARM_MAGIC)`. The kernel's own comment (`agnos/kernel/core/syscall.cyr:8616-8629`) is the correct framing and is quoted rather than paraphrased:

> *"agnos has no per-proc capability/uid yet … so this default-off + explicit-magic-arm is the boundary: a raw disk write can never happen accidentally/incidentally, and the arm call is the exact seam where an aegis installer-capability check lands."*

Through 1.56.55 that comment ended *"when agnos grows per-proc caps"*. 1.56.56 corrected the successor it named: *"agnos is NOT growing per-proc credentials; the gate this token stands in for is a KAVACH capability check in userland"* — the ruling in [`planning/identity-and-authorization-model.md`](../development/planning/identity-and-authorization-model.md) (see §8 below).

⚠️ Mechanically, any ring-3 process can arm it — the magic constant is published in the source. **This is anti-accident, not anti-adversary.** What changed is only how long an arm lasts: through 1.56.50 it was never disarmed; since 1.56.51 `blk_close` clears it (`syscall.cyr:8820-8832`); since 1.57.1 a foreground reap clears it (`proc.cyr:1823-1834, 1914`); and since 1.57.7 (built and gated, not burned) the arm records the pid that set it and clears when that process dies by any path (`syscall.cyr:8632-8641`). `power_sys` says the same thing about shutdown even more plainly (`agnos/kernel/core/power.cyr:410-411`): *"`getuid` is hardcoded 0 and there is no uid model, so a uid check would be a gate that is always open."* — and since 1.56.56 the next lines call that permanent: *"`getuid` returning 0 is the decision, not a gap awaiting one"* (`:412-418`).

### 7. Agent sandboxing — ⛔ absent on AGNOS / 🐧 real on Linux

**This section previously claimed Landlock filesystem sandboxing, seccomp syscall filtering, namespace isolation and cgroup limits, with a Rust code sample. None of it applies to AGNOS, and none of it will: the roadmap rules those mechanisms out permanently.**

kavach is a genuine sandbox **on Linux** — Landlock with the full right mask, seccomp-BPF with `PR_SET_NO_NEW_PRIVS`, namespaces, cgroup-v2, mount-ns + `chroot`. 🐧 On AGNOS every primitive is `#ifdef CYRIUS_TARGET_AGNOS`-gated to a structured "not supported", never a silent success:

Line numbers below are from kavach's tree on 2026-09-27 (`VERSION` 3.13.1, with unreleased 3.13.2 work already in the source — they may move at that tag):

- `kavach/src/security.cyr:198-208` — Landlock returns `kavach_err_not_supported("LANDLOCK")` (`:207`)
- `kavach/src/confine.cyr:108-118` — rootfs confinement returns **−1**, not 0
- `kavach/src/confine.cyr:474-479` — `spawn_seccomp_available` returns 0

kavach's own comments name the reasoning better than a summary can. On the rootfs arm (`confine.cyr:114-116`): *"Returning 0 would report 'the payload is confined to `rootfs`' on a platform where nothing of the sort happened — a sandbox that silently is not one. A caller that cannot confine must be told it cannot confine."* And on `confine_child` (now a wrapper around `confine_child_wd`, comment at `:344-346`): *"Returning 0 here would tell a caller 'the child is confined' on a platform where not one primitive ran — a sandbox that is silently not a sandbox, which is the worst failure mode this file has."* ⚠️ Note also that kavach's `backend_sy_agnos` is inverted from its name — it shells out to docker/podman to run an agnos **container on a Linux host**; it is not agnos-side confinement.

📋 Native confinement is a roadmap item. What exists today, unwired, is kavach's scanner family and HMAC audit chain.

### 8. Permission / identity model — ⛔ absent

**This section previously presented a six-row capability table (`file:read`, `file:write`, `file:delete`, `network:outbound`, `process:spawn`, `agent:delegate`) as a shipped feature. None of those strings exist in any repo.**

- ⛔ **No uid model — and by decision, not deferral.** `getuid` returns 0 for every process (`agnos/kernel/core/syscall.cyr:9667`). There is no `setuid`, no `geteuid`, no `st_uid`. "Restrict to root" is not a sentence AGNOS can write. The ruling in [`planning/identity-and-authorization-model.md`](../development/planning/identity-and-authorization-model.md) (2026-05-12), which agnos 1.56.56 adopted in its kernel comments, is that `getuid`#15 returning 0 *is* the design: identity is meant to come from sigil (the user), t-ron (the agent) and avatara overlays, and authorization from kavach capabilities — not from a per-process uid. None of that is enforced today.
- ⛔ **No per-agent capability enforcement** — not in the kernel, not in daimon (`daimon/src/agent.cyr`'s `AgentHandle` has no capability field), not behind any config schema.
- ⛔ **No human-approval gate.** There is no `request_permission` anywhere. Critical operations do **not** require human approval today.
- ⛔ **No emergency kill switch.** There is no `emergency_kill_switch`. There are no permissions to revoke and no lockdown mode to enter. What a responder can actually do is stop the agent through daimon.
- ⚠️ **shakti is pre-1.0 (0.7.2), Linux-only, and has no kernel seam behind it.** It authenticates against a userland policy file (`/etc/agnos/sudoers.toml`). Nothing stops a process from simply not asking. Applying that ruling, agnos 1.56.56 went further and records shakti as **N/A on agnos by architecture** — a uid-de-escalating sudo equivalent is the shape the identity model rejects; shakti's own roadmap has not absorbed that yet (its `docs/development/roadmap.md:62` still lists 0.8.x AGNOS kernel integration as blocked upstream).

✅ **What ownership checks *do* exist**, verified: `kill`#16 is gated by `proc_may_signal` — init, self, or a direct child only, epoch-validated since 1.57.7 (built and gated, not burned) so a recycled parent slot inherits no authority (`agnos/kernel/core/proc.cyr:1957-1976`); `waitpid`#4 is gated by `proc_may_reap` (`:1990-1995`); `shm_free`#74 is owner-gated; `flock`#59 and the audio band check the calling pid; and since 1.57.7 (built and gated, not burned) every TCP/UDP id is owned by the process that opened it. Those are real, narrow, and worth knowing. ⚠️ Through 1.57.6 `kill` only ever set a pending bit; since **1.57.7** (built and gated, not burned) signal 9 ends the target, 19 stops it, 18 continues it, and `0x100` (`KILL_TREE`) reaches its descendants.

### 9. Audit logging — ✅ shipped as a library, ⚠️ detectable ≠ preventable

libro is a real hash-chained audit chain: SHA-256 hash-linked entries, append-only, constant-time verify, Merkle inclusion and RFC 9162 consistency proofs, per-entry Ed25519 signing with key rotation, three stores (`libro/src/{chain,merkle,proof,signing,file_store}.cyr`). daimon writes agent-lifecycle events into it; argonaut verifies chain integrity when it replays its store on open (`argonaut/src/audit_ext.cyr:121-125`).

⚠️ **Be precise about the tamper model.** The chain makes tampering **detectable**, not **preventable**: the file store is append-only JSONL guarded by `flock`, and on AGNOS every process is uid 0 with no MAC, so any process can truncate or rewrite it. Detection requires an out-of-band copy of the head hash. libro ships anchoring, timestamping and TPM-anchor modules for exactly that; 📋 **none is wired by default** — anchoring and timestamping are compiled into the default bundle but nothing on the append or store path calls them, and the TPM anchor needs `-D LIBRO_TPM` — and per-entry signing is opt-in (`chain_append` never signs).

⚠️ The old revision showed an audit record with a `"user": "alice"` field. **That field cannot be populated** — there is no user identity to put in it.

### 10. Aegis — ⚠️ a library with a stub entry point and no wired consumers

The previous revision described aegis as a running daemon performing real-time behavioural monitoring, automated sandbox enforcement, and coordination with kernel LSMs. Verified reality:

- ⚠️ **Not a daemon.** `aegis/src/main.cyr` sets a log level, prints `"aegis ready"`, and returns — since aegis 1.1.7 it can first open an optional `AEGIS_EVENT_LOG` NDJSON sink. No loop, no socket, no hooks.
- ⚠️ **No repo declares `[deps.aegis]`** — including the consumers its own README names.
- ⚠️ **Scanning is metadata-only** — file exists, size zero, world-writable mode bit, size over 500 MB. No content inspection and no signature check. On AGNOS even the metadata read fails: the scanner's `sys_fstat` is a stub that returns −1 in the agnos stdlib (cyrius 6.6.6 `lib/syscalls_x86_64_agnos.cyr:953`), so every existing file reports `unreadable_metadata`.
- ✅ **Auto-quarantine exists as code** (`aegis/src/lib.cyr:469-540`) — a CRITICAL/HIGH event, if config allows, pushes an entry onto an **in-memory** quarantine list and returns an advisory code. It does not kill, isolate, or terminate anything.
- ⛔ **The "coordination with kernel security modules (Landlock, seccomp, IMA)" bullet is deleted.** There are no LSMs on AGNOS and no LSM framework.
- ⚠️ `aegis/src/firewall.cyr` *renders* an nftables ruleset; its header says the consumer is responsible for applying it. Nothing applies it.

### 11. Phylax threat detection — ✅ real, ⚠️ one-shot scan only on AGNOS

phylax is substantial and genuinely Cyrius-native with no external AV dependency: a YARA-style rule engine with a real `.yar` lexer and parser (`phylax/src/yara.cyr:875,1212,1534`), Shannon entropy and chi-squared analysis (`phylax/src/analyze.cyr:22,79`), magic-byte file typing (`analyze.cyr:119`), PE and ELF parsing, string extraction, script classification, SSDEEP similarity hashing (`phylax/src/hashing.cyr:139`), SARIF reports, a quarantine index, and LLM triage via hoosh. ⚠️ Its `YARA_PAT_REGEX` type is a *simplified* literal search, not a regex engine (`yara.cyr:15`) — do not describe it as regex matching.

- ✅ **Built-in rules** are loaded by `yara_engine_load_builtin_rules` (`phylax/src/yara.cyr:1668`) — `IsPE`, `IsELF`, `UPXPacked`, `NopSled`, `SuspiciousAPIs`, and more. ⚠️ The old claim of "5 default rules (EICAR test, reverse shell, crypto miner, base64 dropper, credential access)" does not match source — there is no EICAR rule.
- ⛔ The old "`.phylax-db` signature database" and "real-time (fanotify)" scan mode **do not exist**. The only `.phylax` artifact in source is a `.phylax-ignore` list.
- ⚠️ **`watch` mode fails closed on AGNOS** with an explicit message — there is no inotify (`phylax/src/cli.cyr:439-448`). Daemon mode is an `AF_UNIX` listener, which AGNOS also lacks. **One-shot `scan` is the only AGNOS path.**
- ⚠️ **No repo declares `[deps.phylax]`.** It is a standalone CLI; nothing calls it in-process, including aegis.

### 12. Sigil trust and package verification — ✅ shipped, at *install*, by ark

sigil is real and broad, all in-house with zero external crypto dependencies: Ed25519, ECDSA P-256/P-384, X25519, the SHA-2 family, HMAC/HKDF, BLAKE2b, Argon2, RSA with constant-time Montgomery arithmetic and verify-after-sign, AES-GCM with AES-NI dispatch, ChaCha20-Poly1305, X.509/PEM/DER, UEFI Authenticode signing.

✅ **Where verification actually happens is at install, in ark** (`ark/src/ark_package.cyr`), before any file is written: magic and format version, a **SHA-256 root hash** over the verified prefix (`:252-259`), an **Ed25519 signature over that root hash** (`:266-272`), bounded manifest/index/payload offsets, a decompression-bomb cap, **per-file SHA-256** against the index, a zip-slip path-traversal guard that rejects the whole package, and a **trust gate** requiring the signer's pubkey to be in the trust set (`:607-617`). (Re-checked at ark 1.4.2; nothing in its changelog since 1.4.1 touches signing or trust.)

⚠️ **Two gates are conditional and must be named:**
- The Ed25519 check runs **only if the package carries the signed flag**. An unsigned `.ark` skips it entirely.
- `require_signed` **defaults to 0** (`ark/src/types.cyr:342`; accessor at `:303`). ark's own pre-v1 audit says why that matters: a signature without a trust anchor proves nothing about publisher identity (`ark/docs/audit/2026-06-18-pre-v1-audit.md:27-28`, residual at `:91-93`).

✅ **One exception, and it is real: marketplace installs force the gate on.** `ark_marketplace_install` calls `acfg_set_require_signed(amgr_config(mgr), 1)` before every install (`ark/src/marketplace.cyr:56-58`), commented *"Marketplace is an untrusted source: always require a trusted signer (fail closed), regardless of the global `require_signed` default."* So a package installed through `ark --marketplace <URL>` **is** anchored to the trust set at `/etc/agnos/ark/trusted_keys` (`ark/src/types.cyr:24`). ⚠️ Only that entry point forces it: a marketplace step executed from a resolved `--apply` plan goes through `exec.cyr:176-200`, which calls `ark_pkg_install_inner` directly and runs under the global default of 0.

⚠️ **A locally-supplied `.ark` is the ungated path, and an operator cannot close it.** `acfg_set_require_signed` has exactly **one** call site in the whole repo — the marketplace one above. `ark/src/cli.cyr` sets only `color`, `apply`, `system_backend`, `apt_wrapper`, `marketplace_url`, `db_path` and `log_path`: there is **no `--require-signed` flag**, and no config-file parser reads the key. The policy is settable in source only. `ark install ./pkg.ark` therefore performs hash verification but **no publisher-identity check**, and nothing an operator can type changes that.

⛔ **Nothing verifies a signature at exec time.** `agnos/kernel/core/elf.cyr:145-200` checks ELF magic, 64-bit class, an entry point ≥ 0x400000, and program-header bounds, with a pre-pass capping every PT_LOAD before allocation (and, since 1.57.2 — built and gated, not burned — refusing an RWX segment). That is the complete exec-time validation — **there is no hash, no signature, and no reference to sigil in the loader.**

⛔ **TPM-backed attestation and transparency-log integration are deleted from this section.** A grep for `transparency` across `sigil/src` and `libro/src` returns nothing; the attestation flow is unbuilt.

### 13. Boot chain — ⛔ no cryptographic verification at all

**gnoboot 0.7.2 performs no cryptographic verification of anything.** Its checks are structural: since 0.7.0's ELF-load hardening it validates the file size, the ELF identity (magic, class, data, `e_type`, `EM_X86_64`), the program-header bounds and **every** PT_LOAD before loading, and re-checks the magic at the load address (`gnoboot/src/main.cyr:507-754`). Through 0.6.x the whole check was a byte compare of the ELF magic and a PT_LOAD type check on the first header (`main.cyr:405-425` then). None of it is a hash or a signature. `gnoboot/cyrius.cyml` declares `[deps] stdlib = ["fnptr"]` — **one stdlib helper and nothing else. sigil is not in the boot path**, so whatever Authenticode capability sigil offers is a library nothing on this path calls.

- 📋 **Secure Boot** is listed under gnoboot's explicit *"post-v1.0 directions — not promises, not committed slots"* (`gnoboot/docs/development/roadmap.md:366-380`). There is no enrolment tool.
- 📋 **TPM / measured boot** — same list, same status.
- ⚠️ **Encryption at rest** is installer-side only: agnova offers a LUKS root, but the AGNOS kernel cannot read an encrypted volume, so this applies to host-staged installs. There is no `--tpm` flag.

✅ What the boot chain *does* give you honestly is **attack-surface reduction**: gnoboot replaces GRUB entirely, removing the multiboot2-EFI relocator that wrote to its own `.text`. That is a reliability and surface win, not an integrity control.

⚠️ **Anyone who can write `\boot\agnos` on the ESP owns the machine, and the ESP is a FAT partition.**

### 14. Post-quantum cryptography — ✅ signing only

**One of the three bullets the old revision listed is real.**

- ✅ **ML-DSA-65 (FIPS 204) signing**, default-on since sigil 3.7.6 (`sigil/src/mldsa_params.cyr:10` — *"Only ML-DSA-65 … is parameterised here"*; `sigil/src/lib.cyr:146-168` — the `-D SIGIL_PQC` flag is now a no-op). ⚠️ "Default-on" means the code is compiled in, not that anything in the supply chain is post-quantum signed. As of 2026-09-27 ML-DSA is called from libro's signing module, argonaut's audit-log keygen (`argonaut/src/audit_ext.cyr:305-317`) and yantra's hybrid pin-descriptor verify (`yantra/src/security.cyr:39-44`); t-ron's source mentions it only in comments. **ark, takumi, nous, mela, bote, daimon, kavach and the kernel contain no ML-DSA reference. Every signature in the supply chain today is Ed25519.**
- ⛔ **ML-KEM / Kyber does not exist** anywhere in the ecosystem (re-swept 2026-09-27 over every repo's `src/`; a bare `kyber` grep matches only "kybernet"). There is no post-quantum key exchange and no hybrid handshake. `sigil/src/x25519.cyr` is X25519 alone.
- ⛔ **SPHINCS+ / SLH-DSA does not exist** in any form.

### 15. Inter-service communication — ⛔ no mTLS; ⚠️ two services with no auth

**The old revision claimed all inter-service communication used mutual TLS with per-agent client certificates from a local CA and zero-downtime rotation. None of that exists.** There is no mTLS implementation, no local CA and no certificate issuance. `sigil/src/certpin.cyr` provides SPKI pinning primitives; nothing consumes them for service auth. daimon, hoosh and bote all speak plain HTTP.

⚠️ **"Nothing else listens" is false.** The verified surface:

| Service | Port | Bind | Auth |
|---|---|---|---|
| daimon `serve` | 8090 | `127.0.0.1` by default since daimon **2.3.0** (its HTTP-listen fix; `daimon/src/server.cyr:260-264`, overridable with `serve --listen`). ⛔ Through 2.2.x it bound **`INADDR_ANY` (0.0.0.0)** and its `127.0.0.1` default (`daimon/src/config.cyr:10`) was never read | ⛔ **none** — daimon's README and `src/api_agent.cyr:11-12` say so, pending its roadmap 2.5.x |
| hoosh `serve` | 8088 | `127.0.0.1` (`hoosh/src/main.cyr:88`; `INADDR_ANY` before hoosh 2.5.1) | ⚠️ **fails open** — `hoosh/src/lib/auth.cyr:35-36` returns 1 when no tokens are configured, which is the default |
| bote http / bridge | 8390 / 8391 | ✅ `INADDR_LOOPBACK` (`bote/src/main.cyr:146,158`) | ⚠️ opt-in only, via `$BOTE_BEARER_TOKENS` (`bote/src/main_common.cyr:97-105`). Unset ⇒ no validator installed. Bridge additionally sets CORS `*`. Default `bote` with no args is **stdio, not a listener** |
| bote-streamable / bote-ws | 8392 / 8393 | `127.0.0.1` | same opt-in |
| agora (telnet BBS) | 2323 | ⚠️ `INADDR_ANY` (`agora/src/main.cyr:36,3186`) | ✅ sigil Ed25519 challenge/response; anon-read, auth-post |
| kavach credential proxy | caller-chosen | `127.0.0.1` (`kavach/src/credential_http.cyr:97`) | ⛔ **none** — a secret-name allowlist only. ⚠️ On AGNOS "loopback-only" is not an isolation boundary: every process on the box is a peer |

⚠️ **The Bind column is what each service asks for — on AGNOS the kernel is not asked.** Through cyrius 6.6.6 the stdlib's agnos `sock_bind` drops the address (`cyrius/lib/net.cyr:295-302`) and `sock_listen`#56 is issued with the port alone (`lib/syscalls_x86_64_agnos.cyr:1633-1641`), so a loopback bind becomes a listener on the NIC's address. agnos 1.57.7 added a loopback-only listen class (built and gated, not burned) that the stdlib does not pass yet; daimon detects the fallback and audits `http.listen.not_loopback` (`server.cyr:1187-1194`).

Several other first-party services also bind `INADDR_ANY`: vidya, hadara and mneme with no authentication found, agnosai with bearer/JWT auth that is **off by default**, and the descent MUD with a per-character passphrase login (see [penetration-testing.md](penetration-testing.md) Target 1).

### 16. Agent / MCP security — 📋 built, not wired into the MCP path

t-ron is a **module set**, not a server — its README says so, and it is confirmed: neither `bote/cyrius.cyml` nor `daimon/cyrius.cyml` declares a t-ron dep, and no `tron_*` symbol appears in bote's or daimon's source (re-checked 2026-09-27). What it *would* enforce there (`t-ron/src/gate.cyr:14-21`): a parameter-size guard, per-agent ACL policy, token-bucket rate limiting, an injection scan, anomaly patterns, cross-agent correlation, and libro audit — with deny codes `unauthorized / rate_limited / injection_detected / tool_disabled / anomaly_detected / parameter_too_large`. Its injection detector runs six patterns (`t-ron/src/safety.cyr:470-492`) over a string with NUL, zero-width, bidi-control, BOM and variation-selector characters stripped (`:339-372`) — a byte-level strip, not NFKC normalization. ⚠️ One consumer exists outside the MCP path: thoth (the agentic coding TUI) vendors t-ron and runs `tron_check` on its own tool calls when `[tron].policy` is set (`thoth/src/gate.cyr:388-438`).

📋 The layered design — input scanning, gateway pre-flight with provenance tags, MCP capability-source policy, irreversible-action confirmation, per-agent capabilities, `UntrustedInput<T>` types — lives in [`docs/development/planning/agent-injection-defense.md`](../development/planning/agent-injection-defense.md), whose own status line reads *"Planning — Design Phase"*. **Cite it as a roadmap, never as a control.**

⚠️ **The concrete unmitigated chain today**: an LLM agent driven by daimon (8090, no auth — loopback on a Linux host since daimon 2.3.0, the NIC's address on AGNOS) can invoke any bote MCP tool (no auth unless a bearer env var is set) with no t-ron gate, no provenance tag, no per-agent capability, no sandbox, and no kernel confinement — and can read any secret from kavach's credential proxy if one is running.

### 17. Zero-trust architecture — 📋 aspirational

Zero-trust is the design target, not the current state. Of its five principles: *"never trust, always verify"* has no authorizer; *"least privilege"* has no privilege levels; *"micro-segmentation"* would require network namespaces, which AGNOS does not have and will not emulate; *"continuous verification"* would require aegis to be a running daemon, which it is not. Only *"assume breach"* is partly met — the libro chain gives forensic detectability (§9).

⚠️ [`docs/adr/adr-003-security-and-trust.md`](../adr/adr-003-security-and-trust.md) is the ADR this section used to cite as authority. **It is itself a fossil** — its decision list names Landlock, seccomp with a "basic 20-syscall filter", and network namespaces. Treat it as a historical record until it is superseded.

## What AGNOS Does Not Have

Named in one place so nobody has to infer it from a silence. All verified absent:

**Kernel**: no uid/gid model · no per-process capabilities · no confinement of any kind · no LSM framework · no loadable kernel modules (single Cyrius binary) · no `ptrace` · no `/proc` · no eBPF · no io_uring · no copy-on-write (`fork`#96 exists since 1.56.54 and is a full address-space copy) · no dynamic linker or `LD_PRELOAD` (static binaries only) · no setuid bit · no `AF_UNIX` · no inotify · no namespaces or `unshare` · no sysctl · `umount`#24 and `mount`#11 are no-op stubs — literally `return 0` (`syscall.cyr:9668` and `:9670`) · no in-kernel packet filter · no TLS in the kernel network path.

**Trust**: no signature verification at exec · no boot-chain verification · no Secure Boot enrolment · no measured boot · no mTLS · no local CA · no transparency log · no post-quantum key exchange · no SPHINCS+.

**Userland**: no PAM on AGNOS · no `sudo` · no systemd · no auditd · no rsyslog · no `/etc/passwd`, `/etc/shadow` or `/etc/group` · no docker · no advisory database or CVE feed for first-party code.

## Security Best Practices

### For operators

1. **Treat the machine as the security boundary.** AGNOS is single-owner and single-trust-domain. Do not run code on it you would not run as root, because you are.
2. **Do not expose the box.** daimon has no authentication and, on AGNOS, listens on the NIC's address whatever it asks for (§15; through daimon 2.2.x it bound `0.0.0.0` on every target); agora binds `0.0.0.0` on 2323. Keep an AGNOS machine off untrusted networks.
3. **Populate the trust anchor at `/etc/agnos/ark/trusted_keys`** with the Ed25519 pubkeys you accept. This is performable today and it *does* bite: marketplace installs force `require_signed` on and will refuse an untrusted signer. ⚠️ **You cannot enable `require_signed` for a local `.ark`** — there is no CLI flag and no config-file key, only a source-level setter (§12). For anything you did not build yourself, install it **through `ark --marketplace <URL>`**, which is gated, rather than by handing `ark install` a local file, which is not — and note that a marketplace step replayed from a resolved `--apply` plan is not gated either (§12).
4. **Copy the audit chain out-of-band** if you care about tamper detection (§9). Concretely: the file store is append-only **JSON Lines**, one record per line, `flock`-guarded (`libro/src/file_store.cyr:1-3`) — so copying the file, or just its last line, to another machine is an ordinary file operation you can do today. ⚠️ **There is no `libro` CLI to print the head hash**: the `libro` binary built from `src/main.cyr` is a **test harness** (hundreds of `assert()` calls, no `argv` handling). The only shipped readout is via daimon's builtin `libro_*` MCP tools (`daimon/src/app.cyr:31`) — which means reaching it goes through the unauthenticated 8090 surface (§15).
5. **Encrypt the root volume at install** via agnova's LUKS option, understanding it applies to host-staged installs only (§13).
6. 📋 Full-disk encryption readable by the AGNOS kernel, TPM sealing, and Secure Boot enrolment are all unbuilt. Do not plan around them.

### For developers

1. **Validate every user pointer with `is_user_range`**, never a bare bounds check — range-form validation is the 1.41.5 audit's standing conclusion, and since 1.56.52 it is what walks the page tables. A length that is `count × stride` with a ring-3 `count` goes through `is_user_array` (1.56.51), because a product the caller already wrapped cannot be caught afterwards. `is_user_ptr` is only `is_user_range(ptr, 1)` since 1.56.52 and has **no production call site**; its one reference is the `shsys` boot self-test that checks the ceiling (`agnos/kernel/core/main.cyr:4432`).
2. **Fail closed and say so.** kavach's AGNOS arms return a structured "not supported" rather than a silent success. Copy that discipline: a control that silently no-ops is worse than one that refuses.
3. **Do not add a syscall band without an ownership check.** The shm band is the standing example of what happens when the check is deferred — and the socket band was, until owners landed at 1.57.7 (built and gated, not burned).
4. **Assume no memory safety.** Cyrius is everything-is-i64 with raw loads and stores; every bound in the kernel is a hand-written `if`, and the automatic stack protector does not exist.
5. **Never document a control you have not opened the source for.** This file exists because that rule was not followed.

### For administrators

📋 There is no fleet-management, SIEM, alerting, or incident-response tooling for AGNOS today. See the *Emergency Procedures* section of [SECURITY.md](../../SECURITY.md) for what a responder can actually do, with the gaps named.

## Vulnerability Reporting & Compliance

For vulnerability disclosure policy, bug bounty, response timelines, compliance status, and contact information, see [SECURITY.md](../../SECURITY.md).

For the dependency and supply-chain process — and for what does *not* exist there — see [vulnerability-management.md](vulnerability-management.md). For the testing checklist, see [security-checklist.md](security-checklist.md).

## References

Primary sources for the claims on this page — read these, not a summary:

Line numbers are as of agnos **1.57.10**.

- `agnos/kernel/core/syscall.cyr` — the syscall dispatcher, the validators (`:318-378`, `:509-633`), the canary (`:1-26`), `getuid` (`:9667`), the block-write arm (`:8590-8832`), the shm band (`:1699-1762`, `:10283-10310`), the owned network band (`:11417-11744`)
- `agnos/kernel/core/proc.cyr` — per-process address space (`:1419`), the KPTI collapse (`:1526-1545`), `proc_may_signal` (`:1957-1976`), W^X mapping (`:2386-2445`), `sys_mmap` and its 1.57.7 memory cap (`:3126-3225`)
- `agnos/kernel/core/elf.cyr` — the only exec-time validation there is (`elf_load` `:145-400`, `elf_load_from_file` `:447-809`)
- `agnos/kernel/arch/x86_64/syscall_hw.cyr` — SFMASK, IBRS, STAC/CLAC, the collapsed-KPTI note
- `agnos/kernel/arch/x86_64/{paging,gdt,idt,boot_shim}.cyr` — the identity map, TSS/IOPB, installed vectors, SMEP/SMAP
- `agnos/docs/development/agnos-userland-abi.md` — the authoritative syscall enumeration (numbers in use as of 1.57.10: #0–#108, with #105 withdrawn)
- `gnoboot/src/main.cyr:482-754` (gnoboot 0.7.2) — the entire boot verification, structural only
- `agnos/docs/development/security-hardening.md` — the S1–S13 record (`:236`, *"closed at v1.28.0"*). ⚠️ Several entries no longer describe the code: S7 KASLR (`:249`) still credits the `pmm_next_free` randomization removed at 1.41.12, S8 KPTI (`:250`) is listed "Partial" but collapsed at 1.40.3, S10 IOMMU (`:252`) says VT-d restricts DMA although VT-d translation never turned on through 1.57.9 (and AMD-Vi is disabled on the iron target), S13 (`:255`) lists `net_handle_tcp` and `net_handle_arp` as canary sites (neither is — `net_handle_tcp`'s went at 1.56.52), invariant 3 (`:17`) predates the HIGH mmap arena, and §9's "no per-process page budget" (`:269-271`) predates 1.57.7's opt-in `spawn_limits`#107
- [`docs/development/planning/kavach-agnos-compat.md`](../development/planning/kavach-agnos-compat.md) — the source-grounded kavach-on-AGNOS audit
- [`docs/development/roadmap.md`](../development/roadmap.md) — the permanent boundary on confinement
- [OWASP Top 10](https://owasp.org/www-project-top-ten/) — applies to the HTTP-serving surface (daimon, hoosh, bote), not the kernel

⚠️ The Linux Security Modules, Landlock and seccomp-BPF references that used to sit here have been **removed**. They pointed at mechanisms AGNOS has permanently ruled out, and a reference list is exactly where a reader goes to learn how something works here.
