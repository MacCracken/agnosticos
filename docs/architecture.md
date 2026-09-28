# AGNOS System Architecture

> **Last Updated**: 2026-09-27
>
> Per-subsystem versions intentionally elided in this doc per the lib-doc precedent — refer to the registry files when quoting numbers.

This document provides the technical architecture of AGNOS (**A** **G**eneral **N**etworked **O**perating **S**ystem, spelling **A-GNOS**). "AI-Native" means the system is *ready for* AI, not *dependent on* it: the kernel, shell, tools, and network all work with zero AI running. AI is an optional layer you turn on or off — a toggleable capability, never a mandatory core.

## Table of Contents

1. [System Overview](#system-overview)
2. [Kernel](#kernel)
3. [Kernel Layers](#kernel-layers)
4. [Named Subsystems](#named-subsystems)
5. [Security Architecture](#security-architecture)
6. [Data Flow](#data-flow)
7. [Technology Stack](#technology-stack)
8. [Design Decisions](#design-decisions)

## System Overview

AGNOS is a sovereign operating system written in Cyrius. The architecture consists of three layers:

```
+======================================================================+
|                        AGNOS Architecture                             |
+======================================================================+
|                                                                       |
|  Named Subsystems (standalone repos, Cyrius-native):                  |
|  +-------+ +-------+ +-------+ +-------+ +-------+ +-------+        |
|  |  ark  | | nous  | |takumi | | mela  | | aegis | | sigil |        |
|  |  pkg  | |resolve| | build | |market | |secure | | trust |        |
|  |  mgr  | |daemon | |system | | place | |daemon | |system |        |
|  +-------+ +-------+ +-------+ +-------+ +-------+ +-------+        |
|  +----------+ +---------+ +-------+ +-------+                        |
|  | argonaut | | agnova  | | kavach| | bote  |                        |
|  |   init   | |installer| |sandbox| |  MCP  |                        |
|  |  system  | |         | |       | |       |                        |
|  +----------+ +---------+ +-------+ +-------+                        |
|       |              |              |              |                   |
+-------+--------------+--------------+--------------+------------------+
|                                                                       |
|  +---------------------------------------------------------------+   |
|  |                     User Space Layer                           |   |
|  |  +-------------+ +-------------+ +------------------------+   |   |
|  |  | aethersafha | |  agnoshi   | |  Agent Applications    |   |   |
|  |  |  (desktop)  | |   (shell)   | | (daimon orchestrated)  |   |   |
|  |  +------+------+ +------+------+ +-----------+------------+   |   |
|  |         +----------------+------------------------+           |   |
|  |                          |                                    |   |
|  |  +-------------+  +-----+------+  +-----------+              |   |
|  |  |    hoosh    |  |   daimon   |  |  agnodrm  |              |   |
|  |  | LLM gateway |  |   agent    |  |  device   |              |   |
|  |  |             |  |  runtime   |  | DRM model |              |   |
|  |  +-------------+  +-----+------+  +-----+-----+              |   |
|  +---------------------------------------------------------------+   |
|                             |                                         |
+-----------------------------+-----------------------------------------+
|                             |                                         |
|  +---------------------------------------------------------------+   |
|  |              AGNOS Kernel (Cyrius-native)                     |   |
|  |  +---------------------------------------------------+       |   |
|  |  |         40+ subsystems · live size in state.md      |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  |  |  Memory   | | Process  | |   Network      |    |       |   |
|  |  |  |  Manager  | | Manager  | |   (TCP/IP)     |    |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  |  |    VFS    | |   SMP    | |   VirtIO       |    |       |   |
|  |  |  | (multi-FS)| |          | | (Net + Blk)    |    |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  |  |  Signals  | |  Pipes   | |  ELF Loader    |    |       |   |
|  |  |  |  + epoll  | | + IPC    | |  + recov. sh   |    |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  |  |   GPU +   | |   HDA    | |  Block + FS    |    |       |   |
|  |  |  |  Display  | |  Audio   | |  (5 backends)  |    |       |   |
|  |  |  +-----------+ +-----------+ +----------------+    |       |   |
|  |  +---------------------------------------------------+       |   |
|  |                          |                                    |   |
|  |  +------------------------------------------------------+    |   |
|  |  |           Hardware Abstraction Layer                   |    |   |
|  |  |      (CPU, GPU, NPU, Memory, Storage, I/O)            |    |   |
|  |  +------------------------------------------------------+    |   |
|  +---------------------------------------------------------------+   |
|                                                                       |
+======================================================================+
```

### Reading this diagram through the maturity lens

The three-layer diagram above is **simultaneously present** in the codebase — every layer has some Cyrius-native existence today. But the **maturity arc** shifts emphasis across the layers as AGNOS matures:

- **demo stage** (**fired 2026-05-25**): kernel layer was the active surface (boot, init, scheduler, syscalls, drivers, ext4 read). Exit trigger was ext2/4 WRITE persisting across a reboot on iron.
- **base stage** (current, post-1.33.x WRITE): kernel reaches "solid". Kernel internals are done; the remaining base work is **ecosystem** — ISO Stage-4, a sovereign ark, soak. Exit trigger is the agnova native installer plus the server ecosystem landing.
- **server stage**: the **Named Subsystems** band fills in — installer (agnova), BBS/MUD apps, server daemons, more libs. Most Linux-shaped workloads become AGNOS-replaceable.
- **desktop stage**: the **Userland Surface** band fills in — aethersafha (native compositor, speaking the sovereign **setu** protocol on the **bhumi** backend — ⛔ not Wayland) + GUI apps. The stage has not *exited* its predecessor, but its work is already landing: the kernel's display/GPU half shipped across 1.54.x–1.56.x, on 2026-08-03 aethersafha composited two real client windows on archaemenid at `cpus online: 4`, and by 2026-08-16 it composited them on the GPU on iron with a pointer, titlebar drag and per-window opacity (⚠ at ~7–15 fps, measured 2026-08-22).
- **swallow stage**: a fourth layer activates — the **compat sandbox**, AGNOS hosting non-AGNOS-native binaries. Named instantiation is **mirshi** (userland syscall-translation supervisor, pico-process model) + **mehman** (compat surface backend); the older "kavach-bounded Linux personality container" architecture is superseded. **The boundary between the kernel and the interpretive layer is permanent** — the kernel never absorbs foreign ABIs (per [[project_agnos_kernel_growth_rules]] / [[project_agnos_empire_defense_layers]]). Sovereignty via universal hosting, not eviction.

The empire-defense architecture (compat / wire / trust / governance) lays out *how the boundaries hold*; the maturity arc lays out *when each capability arrives*. Together they describe both the static structure and the dynamic emergence. Stage definitions and exit triggers live in [`development/roadmap.md`](development/roadmap.md) — cite a stage or a named spec, never a phase number (the Phase 13A–24 numbering was deleted, not merged).

## Kernel

AGNOS runs its own sovereign kernel, written in Cyrius. No Linux dependency at runtime.

**AGNOS kernel** — 40+ subsystems, a small sovereign syscall surface (no BSD socket family / no splice / no AF_ALG). Base kernel-internals are complete; the live head, kernel size and Cyrius pin are in [`development/state.md`](development/state.md). Iron-validated end-to-end on NUC AMD archaemenid: kernel-init layer cleared 2026-05-15 (agnos 1.30.0); closed-beta MVP gate (typeable shell via xHCI HID keyboard) hit 2026-05-18 (agnos 1.30.9); FS-crash-safety (1.37–1.39) + exec-from-disk (1.40.x, ring-3 programs off the agnos-fs); **agnoshi** as the ring-3 userland shell loaded from disk (1.41.x shell-separation arc); **graphics + DOOM playable in-game on iron**; **multi-threading + preemptive scheduling + SMP**; **HDA audio — DOOM-with-sound out the analog front jack (1.52.x)**; kernel **FP/SIMD — real f64 in ring 3 (1.53.x)**; **GPU compute on the Cezanne shader cores with no amdgpu and no ROCm (1.54.x)**; **display / scanout + a sovereign ATOM BIOS interpreter + ACPI S5 self-poweroff (1.55.x)**; and **shader compositing, a GPU triangle rasteriser, native-resolution modeset and a hardware-panned console (1.56.x)** — all iron-validated on real hardware, and on **2026-08-03** the aethersafha desktop hosted two real client windows on that box at `smp: cpus online: 4`. The latest iron burn is **1.57.1** (2026-09-08); 1.57.2 → 1.57.10 are built and gated, not burned:
- Memory management, process management (incl. `fork` and process enumeration, 1.56.47–1.56.55), SMP
- TCP/IP networking, VirtIO-Net/Blk
- ext2/ext4 + FAT12/16/32 + exFAT filesystems (read+write), ELF loader (streaming, ring-3 exec-from-disk)
- Pipes, signals, epoll, timerfd, shared memory, and the local-IPC channel band (`#97`, 1.56.40)
- Recovery-REPL shell (the interactive shell moved to userland `agnsh` in the 1.41.x shell-separation arc)
- kybernet as PID 1
- Sovereign UEFI handoff (Path C, RDI = `&boot_info` via gnoboot)
- Native XHCI + USB-HID-boot keyboard driver (all 5 phases landed; the archaemenid silent-absorb arc closed at the 1.30.9 MVP gate — root cause was an upstream Cyrius gvar-init-order bug, not silicon), plus the boot mouse since 1.56.42 — the same cut deleted PS/2 / i8042 from the kernel, so keys arrive only through xHCI HID
- HDA audio out (`snd_*` band), FP/SIMD (per-core SSE + per-proc FXSAVE)
- GPU compute, display scanout, CP-DMA 2D, shader compositing and DCN modeset — see *Beyond the Five Layers* below
- Built and QEMU-gated, **not burned**: a kernel-embedded TrueType face (1.57.2), per-process kernel stacks and the blocking model (1.57.6–1.57.9), and VT-d translation (1.57.10 — QEMU q35 only; the AMD iron target runs with AMD-Vi disabled)

The `kernel/` directory in this repo contains Linux kernel configs for **host bootstrap only** — building the cross-compiler toolchain on an existing Linux host before AGNOS can self-host.

## Kernel Layers

> Conceptual decomposition. Layers 1–5 shipped. Live per-subsystem status: [agnos repo](https://github.com/MacCracken/agnos).

The kernel was originally planned as five layers — *boots*, *runs programs*, *storage*, *talks to the world*, *usable*. All five landed inside a ~7-week window (Cyrius scaffold 2026-04-03 → kernel v1.22.0 on 2026-04-14, hardened to v1.26.1 by 2026-04-28). Subsequent 1.27.x → 1.30.x work has been bring-up and hardening: KASLR data-only shipped at 1.28.0 (closed the security-track gate 13/13; its allocator half was removed at 1.41.12 — [security-guide.md](security/security-guide.md) §5), sovereign-struct kernel ABI shipped 1.30.0 (Path-C UEFI handoff via gnoboot v0.5.0), native XHCI + USB-HID-boot driver across 1.30.1 → 1.30.5, xHCI cmd-path repair arc 1.30.6 (Repairs FF through QQ, MSI-X table programming closeout). Everything after that grew the layers rather than adding a sixth: storage (1.31.x–1.40.x), networking (1.32.x–1.35.x), SMP + preemption (1.44.x–1.46.x), and then a whole capability band the original plan never scoped — audio, FP/SIMD, GPU, display and modeset (1.52.x–1.56.x), collected under *Beyond the Five Layers* below. The decomposition below reflects where each layer sits today.

### Layer 1 — Can Boot and Respond

**x86_64**: multiboot1 32-bit ELF entry, 32→64 long-mode shim, GDT (5 segments + TSS), IDT (256 vectors), PIC remap to INT 32+, Local APIC at `0xFEE00000` (timer, IPI), periodic ~100Hz timer, PS/2 keyboard (full US QWERTY — ⛔ deleted at 1.56.42; keys now arrive only through xHCI HID), COM1 serial I/O at `0x3F8`.

**aarch64**: DTB boot, EL2→EL1 transition, GICv2 interrupt controller, ARM generic timer, PL011 UART. ⛔ The aarch64 kernel does not compile as of 1.57.9 (agnos `state.md`: the stubs have not kept up with `core/`); the port is its own future arc, not maintained in lockstep.

**Memory**: page tables (2MB huge, per-process; the five-layer cut's 16MB identity map is now a 256 MB identity window plus a direct map of RAM at 8 GB), PMM (bitmap — 4096 pages at the five-layer cut, now sized to installed RAM; 4 KB pages are allocated top-down since 1.41.12 rather than from a next-free hint), VMM (map/unmap/alloc, user-accessible), slab heap (8 size classes, 32B–4096B).

### Layer 2 — Can Run Programs

- **ELF loader** — static ELF64, per-process address space, Cyrius-emitted binaries load directly
- **Process table** — 16 slots, CR3 per-process
- **Context switch** — full register save/restore (all 9 caller-saved regs) + CR3 switch
- **Scheduler** — round-robin on timer tick
- **SYSCALL/SYSRET** — MSR setup, ring 3 ↔ ring 0 transition
- **TSS** — RSP0 for ring 3 stack switching
- **Per-process kernel stacks + the blocking model** (1.57.6–1.57.9, built and QEMU-gated, **not burned**) — every process runs its syscalls on its own kernel stack, and a wait (the keyboard line, `waitpid`, `sleep_ms`, `flock`, pipe and channel reads, pipe writes) blocks only its caller instead of holding a CPU

### Layer 3 — Can Access Storage

The original 5-layer plan listed VirtIO-Blk + FAT16. The 1.31.x cycle (Mar–May 2026) substantially expanded this layer into a full storage stack on real silicon. Five iron debuts on archaemenid (Beelink SER, AMD Cezanne).

**VFS layer:**
- **VFS** — file table of typed fds: device, memfile, signalfd, epoll, timerfd, pipe, ext2 file (1.31.5 Phase 3) and ext2 directory, FAT/exFAT write-back, socket (1.49.4) and channel endpoint (1.56.40) — the tag list is `kernel/core/vfs.cyr`
- **Initrd** — flat format, name lookup; bare-name fallback for shell verbs after CWD-prefixed ext2 lookup miss

**Block-layer dispatch (5-backend, multi-backend probe):**
- **`block.cyr`** — priority-order tag dispatch: NVMe primary > AHCI secondary > USB MS tertiary > VirtIO fallback > RAM-disk
- **`blk_registered` bitmask** + **`blk_read_on(tag, sector, buf)`** — explicit per-backend dispatch independent of `blk_active`; lets the filesystem probe walk all registered backends instead of just the active one (1.31.6 bite G)

**Storage device drivers:**
- **NVMe (Phase 1-5)** — probe + admin queue + I/O queue + R/W + PRP1/2/list dispatch. Iron-validated on Crucial P3 2 TB (1.31.0, first-try clean)
- **AHCI/SATA (Phase 1-4)** — HBA probe + per-port CL+FIS bring-up + IDENTIFY DEVICE + READ/WRITE DMA EXT. Iron-validated on WD Blue SA510 2 TB (1.31.1); three carry-forward patches (port quiescence gate / ATA-string right-trim / RW_DEMO compile gate) landed in 1.31.2
- **USB Mass Storage (Phase 1-2.8)** — BBB transport + SCSI INQUIRY/TUR/RC10/READ(10)/WRITE(10). Eight-bug repair stack (bulk-timeout extension + strict TRB matching + SHORT_PACKET residue check + unified retry-recover wrapper + drain reposition + 64-bit param_hi). Iron-validated on Silicon Motion 125 GB stick (1.31.3)
- **VirtIO-blk modern (1.x)** — full PCI cap-list + MMIO + 64-bit FEATURES_OK + polled used-ring rewrite (1.31.4); replaces the legacy 0.9.5 transitional driver. QEMU-only by design
- **RAM-disk** — `pmm_alloc`-backed, `RAMDISK_ENABLE=1` compile-flag gated (1.31.4)

**Partition + filesystem layer:**
- **GPT (Phase 1-3)** — header + full 16 KB array walk + UTF-16LE partition names + table-less CRC32 + backup-header recovery + 7-GUID type classifier + `parts` shell command. Iron-validated multi-partition layouts on real hardware
- **ext2 / ext4 read + write + extent allocation + JBD2 journaling** — superblock + BGDT + inode (direct + single/double/triple indirect tree) + dirent walk + absolute-path resolution + `VFS_EXT2_FILE` FD type + ext4 extents header/leaf walker (FreeBSD-shape) + 64BIT support (s_desc_size + dynamic BGDT stride + bg_inode_table_hi guard) + **write path** (create / write / unlink / mkdir / rmdir / rename / ln / symlink / truncate, metadata_csum-stamping, `e2fsck -fn`-clean — 1.33.x WRITE arc) + **ext4 extent allocation** (depth-0 append → depth-0→1 grow → multi-leaf depth-1 sibling split → depth-1→2 index-block grow, the full on-demand grow ladder — 1.37.x) + **JBD2 journaling** (probe → log reader → replay-on-mount → in-memory transaction lifecycle → write path with 3-barrier sync-checkpoint → `put_inode` integration → crash-injection smoke + hardening — 1.38.x). **Iron-validated** on real Linux ext4 on NVMe (1.31.6 read-only baseline; 1.31.7 ext4 64BIT shell UX; 1.33.1 write-survives-reboot demo→base exit; 1.37.3 extent allocation depth-2)
- **Partition-aware mount via GPT consumption** — `ext2_try_partition_mount` iterates Linux-FS-GUID partitions when whole-disk probe misses; mounts the first match
- **fatfs (FAT12/16/32 read + write)** — partition-aware multi-backend mount + FAT-chain traversal + create / content / delete / truncate + LFN. `fsck.fat -n`-clean (1.34.x FAT-family arc)
- **exFAT (read + write)** — allocation bitmap + typed dir-set (SetChecksum / NameHash) + up-case table for Unicode names + directory growth (root extension + spanning dir-set append). `fsck.exfat -n`-clean (1.34.x)
- **FS write-safety** — ESP-write guard at 1.34.6: FAT/exFAT writes refused on boot-ESP partitions (firmware territory); data writes go to MSFT-Basic partitions / USB sticks

**Multi-source convergent prior-art audits**: every non-trivial subsystem in this layer gets a written audit before its iron burn. Linux is one source of many — FreeBSD / OpenBSD / NetBSD / Haiku / EDK2 / SeaBIOS / U-Boot / Plan 9 + vendor errata triangulated.

**Not yet shipped at this layer**: HTREE indexed directories, ext4 hole-fill + out-of-order extent inserts, JBD2 full revoke handling, bitmap/group-desc/extent-tree journaling (1.38.x integration narrowly routes the inode-table write only), NTFS / squashfs read, optical via USB-MS SCSI MMC (non-512-B sectors). Framebuffer/MMIO graphics belongs to the display band, not Layer 3 — see *Beyond the Five Layers*.

### Layer 4 — Can Talk to the World

- **r8169 GbE NIC driver** — RTL8168/8125 PCI probe + RX/TX rings + RX-ring deepen 16→64 (1.32.7 unicast-RX fix). Iron-CONNECTED on archaemenid (1.32.7). The 1.32.x networking arc closed with DHCP `.142` real-lease iron-verified at 1.32.9.
- **VirtIO-Net** — legacy PCI, virtqueues, Ethernet frames (QEMU-only; back-burnered with known TX gap)
- **ARP + IPv4 + UDP** — full send/recv; UDP 8-listener bind table
- **TCP** — connect/send/recv/close with SYN/ACK/FIN state machine + server primitives (listen/bind/accept) + 1.35.1 hardening (in-order receive ring, retransmit/RTO, MSS/segmentation, peer-window)
- **DHCP client** — RFC 2131 DISCOVER → OFFER → REQUEST → ACK; real lease iron-verified (1.32.9)
- **DNS stub resolver** — `dns` shell verb, RFC 1035 parse incl. compression pointers, 8-entry TTL-respecting positive cache (1.35.0 / 1.35.6)
- **ICMP echo** — `ping` verb, pingable + outbound (1.35.0)
- **NTP/SNTP** — one-shot SNTP query for wall clock, `ntp` + `date` verbs (1.35.2)
- **RTC boot clock** — CMOS read at boot for wall clock without network, `date [RTC]` / `[NTP]` (1.35.5)
- **Arc-close ingress hardening** — `net_poll` clamps attacker-controlled IPv4 total-length to actually-received frame size, closing forged-length over-read on ICMP/UDP/TCP handlers (1.35.7)
- **Owned sockets** (1.57.7, built and QEMU-gated, **not burned**) — every TCP/UDP id belongs to the process that opened it; `sock_peer`#106 reads the remote address of a connection the caller owns
- **Serial console** — input + output via COM1 / PL011

### Layer 5 — Can Be Used

- **Shell** — As of the **1.41.x shell-separation arc** (iron-validated June 2026 — `agnoshi` runs in ring 3 off the disk on archaemenid), the full interactive shell is the **userland `agnsh`** (agnoshi binary, loaded from `/bin/agnsh` on the ext2 root and run in ring 3). The in-kernel shell shrank to a **recovery-only REPL** (`kernel/user/shell.cyr`, dropped 1149 → 813 LOC at 1.41.9 when the non-recovery verbs were deleted) that runs as the fallback when `/bin/agnsh` is absent. The recovery REPL retains a curated diagnostic/repair subset: `help echo uptime klug` (base; `klug` dumps the kernel log ring as a builtin, so it needs no exec — `test` exists only in TEST builds) + `halt reboot poweroff` (real power stops since 1.56.60 — until then `halt` only exited the REPL and `reboot`/`poweroff` did not exist here) + `cat ls cd pwd rm mv sync run` (FS read/repair + exec-from-disk, mount-routed across ext2/FAT/exFAT) + `blkread disk parts lspci cpus` (block/partition/hardware inspection) + `net send recv tcp pipe dns ping ntp date` (network diagnosis). The heavier interactive surface — `ps free bench`, the `mkdir rmdir touch ln` FS-mutation verbs, the `jbd2` journal diagnostic — moved out of the kernel into `agnsh`. `ls` accepts flag tokens (`-la` no-op for now); `cat` falls through to initrd on ext2 miss; `cd` + `pwd` consume `sh_cwd_inode` / `sh_cwd_path` globals for CWD-relative path resolution (1.31.7).
- **kybernet PID 1** — service supervision, signal/event-loop, kernel-interface boundary. Per-repo benchmarks + test count in kybernet's own state.md.
- **Signals** — per-process `proc_signals` / `proc_sigmask`, `kill` / `sigprocmask` / `signalfd`. Since 1.57.7 (built and QEMU-gated, not burned) `kill` acts: 9 ends the target, 19 stops it, 18 continues it; every other signal only sets a pending bit
- **Epoll** — `epoll_create`, `epoll_ctl`, `epoll_wait`
- **Timerfd** — `timerfd_create`, `timerfd_settime`
- **Pipes** — circular buffer IPC, read/write ends, VFS type 6. Streaming since 1.56.40: an empty pipe with a live writer answers WOULD_BLOCK, and 0 means only EOF; since 1.57.8 a read without O_NONBLOCK blocks instead, and since 1.57.9 so does a write to a full pipe

### Beyond the Five Layers — Audio, FP/SIMD, GPU, Display

The original plan stopped at *usable*. Everything below was scoped after it and now constitutes the largest single band in the kernel by module count. The iron target throughout is **archaemenid** — a Beelink SER NUC, AMD Cezanne APU (PCI `1002:1638`), gfx90c compute + DCN 2.1 display. ⚠ **QEMU is structurally blind to this band**: there is no Cezanne iGPU to match, so every GPU branch is dead for a whole QEMU run. A green QEMU boot proves the CPU fallback and nothing about the GPU.

- **HDA audio (1.52.x)** — Azalia/HDA controller driver and the `snd_open/config/write/close/drain/avail` ring-3 band. **Iron-validated**: DOOM-with-sound out the analog front jack. ⛔ **HDMI audio is SHELVED (2026-08-11)** by operator decision — no burn through the shelving produced sound, although every source-side observable reads PLAYING; the register-poke class of fix is closed (agnos reached byte-identity with the amdgpu answer key and the sink stayed silent), and sequencing is **not** eliminated. Do not read "audio works" as covering the display path.
- **FP/SIMD (1.53.x)** — `arch/x86_64/fpu.cyr`: per-core SSE enablement plus per-process FXSAVE state with lazy `#NM` save/restore, so two f64 processes cannot corrupt each other's XMM. **Iron-validated** — real `f64` in ring 3. The production kernel itself stays FP-free by invariant; the f64 proofs live behind a selftest gate.
- **GPU compute (1.54.x)** — PSP GPCOM ring-create and `SETUP_TMR` (the first DMA round-trip), the CP/MEC/RLC firmware set loaded through PSP, GFXHUB GMC setup, a mapped compute queue, PM4 packets and doorbell, then hand-assembled gfx90c shaders on the shader cores. Integer tiled matmul and full-precision f64 matmul both bit-correct against the CPU, and **rosnet-bit-correct including rounding** because both sides sum k-ascending. Exposed to ring 3 as `#82 gpu_dispatch` / `#83 gpu_dispatch_f64`. **No amdgpu, no ROCm.**
- **Display / scanout (1.55.x)** — read-only DCN 2.1 live-pipe probe → the first DCN write → the scanout flip → vblank pacing → a double-buffered present loop paced by the display's own clock. Ring 3 sees it as `blit`#39's `defer` bit plus `present`#84: tear-free full-screen apps with **no app change**. The AMD-Zen quiet-boot banding was root-caused here as a surface/raster **scale** mismatch (an 800×600 surface DCN-scaled to 2560×1440), fixed read-only by matching `fb_console`'s geometry to the real HUBP viewport — ⛔ it was neither tiling nor DCC.
- **Sovereign ATOM BIOS interpreter (1.55.23/24)** — `core/atom.cyr`, a faithful Cyrius port of the ATOM bytecode VM. The VBIOS image is acquired from the ACPI VFCT table (Cezanne is an APU, no discrete flash ROM) and the interpreter drives `DIGxEncoderControl` and `DIG1TransmitterControl`. **Iron-proven bit-correct**: the encoder and transmitter runs emitted exactly the reference oracle's write sequences.
- **Orderly shutdown + ACPI S5 (1.55.25/26)** — `core/power.cyr` with `core/acpi.cyr`: a durability barrier, then device quiesce across storage / net / USB / audio / GPU, the FADT + `\_S5` decode, reset, and poweroff. **Iron-validated** — `poweroff` at the agnsh prompt takes the machine down and the power LED goes out.
- **Hardware 2D — CP-DMA (1.55.30)** — a 7-dword PM4 `DMA_DATA` on the proven MEC compute ring gives copy, constant-fill and true strided blit. **Iron-verified** in one boot, the blit proven by leaving the destination's inter-row padding untouched. ⛔ **SDMA is PARKED** — it rings up on iron but never fetched a packet; an agnos setup gap, not a hardware limit.
- **The GPU compositor seam (1.55.32)** — `#86 shm_create_gpu` (a page from the GPU carveout, because a `#71` shm page is system RAM the GPU cannot reach at all), `#87 gpu_blit_shm`, `#88 gpu_fill_rect`, `#89 gpu_caps`, with `#90 gpu_readback_shm` and `#91 gpu_blit_bb` alongside. **Iron-proven**: a whole mock compositor frame with **zero per-pixel CPU work**.
- **Shader compositing and 3D raster (1.56.x)** — the machine-assembled gfx90c shader programs live in `kernel/shaders/` as `.s` sources, a category of kernel artifact that did not exist before 1.54.14. They are reached through **one** descriptor syscall, `#92 gpu_shader_op`, carrying an array of 64-byte op records with the opcode inside the payload — new ops need no new syscall number. The live op codes — `NOP`, `BLEND_RECT`, `BLEND_COV`, `GLYPH_1BPP`, `GRAD_LINEAR`, `BLEND_ALPHA` (op 0x06, 1.56.44; burned 2026-08-16 — a translucent window composited on iron), `EDGE_COV`, `TRI_RGBA`, `TRI_LIST`, `TRI_TEX`, `TEX_LIST`, `DEPTH_CLEAR`, `TRI_DEPTH`, `TRI_PERSP`, `RT_READ` — each have a worker (`GPU_OP_SUPPORTED` in `core/syscall.cyr` is the authoritative set). The rung ladder built from there to a **GPU triangle rasteriser** (20 of 20 cases byte-identical to the CPU reference), barycentric RGBA interpolation, texturing with WRAP / FULLCOV / COLMAJOR / bilinear, depth clear and depth test, and perspective-correct texturing — each closed on iron.
- **Modeset (1.56.10 → 1.56.38)** — `#93 gpu_modeset_op`, deliberately a different number from `#92` because modeset is a distinct capability class. Write ops sit behind an **arm-once ext2 latch** (`core/modeset_latch.cyr`, `/.modeset-armed`) so a display write that blanks the console non-recoverably costs exactly one bad boot and never a reflash; the latch releases itself at clean shutdown. `core/mode_raster.cyr` computes the raster program from a mode description and **writes no register**, which is what lets a host test include the real file rather than a copy of it. `MDO_OP_NATIVE` retargets the pipe to **2560×1440 with the scaler in DSCL bypass**; `MDO_OP_PAN` scrolls the console by moving the scanout start instead of copying 14.7 MB. **Released and burned PASS**: `Timer ticks before sched` 28 (800×600) → 149 (native, software scroll) → **11** (native + pan): native *without* the pan is the regression, so the two ship as one change. It says nothing about the pan alone, which cannot be built — `mdo_pan` refuses unless the pipe is already native.
- **The desktop's kernel half (1.56.35, iron 2026-08-03)** — the AP trampoline set `EFER |= LME` only where the BSP sets `LME|NXE`, so with NXE clear bit 63 of a paging entry is *reserved* and every W^X data page and user stack faulted on an AP. With that fixed and a cross-CPU TLB shootdown added, archaemenid boots to `smp: cpus online: 4` and the **aethersafha compositor hosts two real client windows** — setu's `present_probe` and `crab`'s dual-pane file manager — composited on the panel, 278 frames, keys delivered, clean Esc quit. ⚠ Scope: that run used the **CPU blit path**; the GPU composite path was iron-proven separately for a single opaque surface. Since then, on iron: the clients ride the `#97` channel band (2026-08-07), every frame layer is on the GPU with the `#39` blit gone (2026-08-08), a USB-mouse pointer through `#98` and a titlebar drag (2026-08-09), and per-window opacity through `#92` op 0x06 (2026-08-16). ⚠ The first frame-cost measurement on iron (2026-08-22) read **~7–15 fps**, and the background clear is not the cause — aethersafha's `state.md`.

### Size — Projected vs Actual

| Milestone | Original estimate (Apr 7) | Actual |
|-----------|---------------------------|-------|
| Layer 1 alone | 31 KB | 31 KB ✓ |
| + Layer 2 (run programs) | ~38 KB | — |
| + Layer 3 (storage) | ~40 KB | — |
| + Layer 5 (shell) | ~42–45 KB | — |
| + VFS + signals + pipes | ~50 KB | — |
| + VirtIO + basic TCP | ~65 KB | — |
| Full Layer 1–5 + Path-C UEFI ABI + USB-HID + xHCI cmd-path | ~70–80 KB | **live size drifts across the cycle** |

The shipped kernel is several times the original estimate because it carries features outside the original 5-layer shortest-path plan: SMP infrastructure (APIC, IPI, trampoline, per-CPU stacks); cross-architecture source (x86_64 plus an aarch64 port in one tree — ⛔ the aarch64 build does not compile as of 1.57.9); full TCP + DHCP + DNS + NTP + ICMP comms substrate (1.35.x) instead of UDP-only; slab allocator with 8 size classes; full FAT12/16/32 + exFAT read+write drivers (1.34.x) instead of FAT16 read-only; ext2/4 read **+ write + extent allocation + JBD2 crash-safe journaling** (1.33.x–1.38.x); Ring 3 with proper TSS; Local APIC timer; v1.30.x's Path-C sovereign UEFI handoff and native xHCI / USB-HID-boot driver added another sizable chunk; the 1.40.x exec-from-disk arc added the streaming ELF loader + ring-3 execution + process model (create/reap/waitpid). The 1.37.5 kashi vendoring (~+100 KB) brings the freestanding font-data core (full CP437 + 8x8 + 9x16-derived faces); the 1.38.x JBD2 stack (~+30 KB) adds the full probe → log reader → replay → write path → integration → crash smoke surface. The 1.52.x–1.56.x band — HDA audio, FP/SIMD state, the GPU register/firmware/ring stack, the ATOM interpreter, DCN display and modeset, and the gfx90c shader programs compiled in — is the largest single addition since; it took the binary past 1.4 MB at 1.54.1, past 1.6 MB at 1.55.25 and past 1.7 MB at 1.56.5. Live size: [`development/state.md`](development/state.md).

The conceptual 5-layer model still maps. The kernel simply carries more per layer than the MVP "boots into shell" scope intended.

### Syscalls

The surface was 34 calls (0–33) at the 1.41.x shell-separation cut and has grown as ring 3 took over more of the system — at 1.57.10 the numbers run 0–108, with `#105` withdrawn. The live number set and the per-call contracts — arguments, blocking behaviour, error conventions and the Linux number collisions to watch for — live in the agnos repo's [`agnos-userland-abi.md`](https://github.com/MacCracken/agnos/blob/main/docs/development/agnos-userland-abi.md). This doc names the bands only:

| Band | Numbers | Content |
|------|---------|---------|
| Core process / FS / IPC | 0–33 | the original surface — exit/write/getpid/spawn/waitpid/read/close/open/dup, mkdir/rmdir/mount/sync/umount, reboot/pause/getuid, kill/sigprocmask/signalfd, epoll ×3, timerfd ×2, pipe, `write_boot_checkpoint` (the CMOS iron-boot diagnostic slot), mmap/munmap (anonymous, 2 MB-granular), getdents/unlink/rename/link/stat (the 1.41.3 group `agnsh` needed) |
| Identity + kernel log | 34–36 | `uname`, `sysinfo`, `klug` (the unified kernel-log ring, dmesg-shaped; since 1.56.58 every kernel-origin line carries a Linux-style uptime prefix, and ring-3 output passes through unstamped) |
| Exec + framebuffer | 37–39 | `execwait` (run-to-completion in ring 3), `fbinfo`, `blit` |
| Timing + raw input | 40–42 | `uptime_ms`, `sleep_ms`, `kbscan` (non-blocking scancode drain — games need key up/down, not cooked lines) |
| Concurrency | 43–44 | `spawn_path` (non-blocking from-disk spawn), `sched_yield` |
| Entropy + wall clock | 45–46 | `getrandom` (RDRAND), `time_unix` (RTC/CMOS) |
| Network | 47–57 | TCP client + server, UDP, ICMP echo |
| File + terminal extras | 58–63, 70, 81 | `lseek`, `flock`, `winsize`, `net_config`, `exec_redirect`, `symlink`, `readlink`, `readdir` |
| Audio | 64–69 | `snd_open` / `snd_config` / `snd_write` / `snd_close` / `snd_drain` / `snd_avail` — HDA output |
| Shared memory | 71–74 | `shm_create` / `shm_write` / `shm_read` / `shm_free` |
| Raw block (installer) | 75–80 | `blk_enum` / `blk_open` / `blk_read` / `blk_write` / `blk_info` / `blk_close`; RW is capability-gated, not merely requested |
| GPU + display | 82–94 | compute dispatch (`#82`/`#83`), present + fill (`#84`/`#85`), the compositor seam (`#86`–`#89`), readback + back-buffer blit (`#90`/`#91`), the shader-op descriptor seam (`#92`), the modeset seam (`#93`), GPU hang/recovery (`#94`) |
| Clock | 95 | `uptime_us` — microsecond monotonic, readable with interrupts disabled; returns −1 rather than a plausible 0 when calibration was refused (calibrated against the ACPI PM timer since 1.57.6) |
| Process | 96, 99, 107, 108 | `fork` (1.56.54, corrected 1.56.55 — a full copy of the caller's address space, not copy-on-write), `proclist` (process enumeration, 1.56.47), `spawn_limits` (one-shot memory / CPU caps on the caller's next child, 1.57.7), `sched_yield_to` (a directed yield, 1.57.9) |
| Local IPC | 97 | `chan_op` — the channel band that replaced TCP-on-loopback as the display transport (1.56.40): mint / send / receive / close / endow on one op-dispatched number; since 1.57.8 a blocking receive is `read`#5 on the endpoint fd |
| Pointer | 98 | `ptrscan` — one merged USB-mouse sample per call (1.56.42); a 20-byte record carrying the wheel delta is opt-in by buffer size (1.56.49) |
| Network extras | 100, 106 | `icmp_echo_ex` (ICMP echo with a caller-chosen deadline, 1.56.48), `sock_peer` (the remote address of a connection the caller owns, 1.57.7) |
| File + volume | 101–104 | `readdir_at` (a directory listing that can resume, 1.56.50), `lstat` (1.56.53), `statfs` (1.56.56), `mountlist` (1.56.59) |

`#96` (`fork`) and `#97` (`chan_op`) were assigned by the operator on 2026-08-05. ⚠ Through 1.56.40 this section carried `#97` as in-flight and `#96` as reserved; both have since shipped. `#97` landed with the 1.56.40 channel band, its Cyrius stdlib peer `SYS_CHAN_OP` followed, and it ran the desktop on iron on 2026-08-07 (1.56.41 kernel). `#96` shipped at 1.56.54 and was corrected at 1.56.55. `#105` (`blkstats`) was minted during the 1.56.59 cut and withdrawn in it — its per-device counters ride `sysinfo`#35's tail instead — so the number stays unassigned. A kernel number can ship before its Cyrius stdlib peer: at 1.57.10 `#106`–`#108` have none yet, so agnos's syscall-ABI gate is red by design, and ring 3 calls them by raw number meanwhile.

⚠ `#44 sched_yield` and, since 1.56.54, `#96 fork` are dispatched from the ring-3 SYSCALL entry stub — `syscall_handler` in `arch/x86_64/syscall_hw.cyr` — and have no `ksyscall` arm: an in-kernel `ksyscall(44)` falls to the default −1 (kernel code that wants to yield calls the scheduler directly), and a forked child resumes from the caller's own syscall frame, which exists only on a path reached from the entry stub. A count of `ksyscall` dispatch arms therefore comes up short of the minted numbering; the gap is the entry-stub dispatch, not a missing call (agnos's `syscall-abi-check.sh` lists these calls as `ENTRY_STUB_ONLY`). `#14 pause` also yields there first but still has its own `ksyscall` arm, so it does not widen the gap.

This remains a small sovereign surface. The network band is a **fixed TCP/UDP/ICMP set with slot-indexed connections** — there is no generic BSD socket family, no `socket()` over arbitrary domains, **no splice, no AF_ALG**. Several GPU numbers collide with unrelated Linux calls (`#90` `chmod`, `#91` `fchmod`, `#92` `chown`, `#95` `umask`); a file-level `#ifdef CYRIUS_TARGET_AGNOS` gate in the Cyrius stdlib is the only barrier off-agnos.

### Userland Alignment

| Kernel provides | Userland uses |
|-----------------|---------------|
| ELF loader (Layer 2) | Cyrius compiler emits ELF directly; no translation layer |
| Initrd + VFS (Layer 3) | Userspace tools packed into CPIO initrd at build |
| Signals + epoll + timerfd (Layer 5) | kybernet event loop, service supervision |
| SYSCALL/SYSRET (Layer 2) | cyrius stdlib per-target syscall layer (`syscalls_*.cyr`); the bindings moved here from agnosys in the agnos→agnodrm decomposition — agnodrm is now the device/DRM model, not the syscall layer |
| Pipes (Layer 3/5) | Shell pipelines, inter-service IPC |
| Local-IPC channel band (`#97`) | aethersafha spawns each client already holding a connected endpoint; a terminal's shell stdio rides a channel as its PTY |
| Framebuffer + GPU seam (`#38`/`#39`/`#84`–`#94`) | cyrius-doom draws through `fbinfo` + `blit`; aethersafha composites client surfaces out of GPU-visible shared memory with no per-pixel CPU work |

### What's not yet in the kernel

| Gap | Why it's deferred |
|-----|-------------------|
| Framebuffer text rendering quality (Quiet-Boot legibility on AMD Zen) | ✅ **Resolved + iron-validated (1.55.28 → 1.56.38)** — root cause was a surface/raster **scale** mismatch (an 800×600 firmware surface DCN-scaled to 2560×1440), not tiling and not DCC; the OSDev #57150 thesis is falsified. Fixed read-only by matching `fb_console` to the real HUBP viewport, then superseded outright: `MDO_OP_NATIVE` gives a real 2560×1440 scanout with the scaler bypassed, `MDO_OP_PAN` a hardware-scrolled console, and 1.56.37's deferred first paint removed the pre-aperture band. **Burned PASS.** ⛔ The GOP-side `SetMode` lever is finished — but not because "AMD Zen UEFI elides SetMode"; gnoboot 0.6.1's `SetMode` demonstrably works and the firmware allocates a real 2560×1440 framebuffer. `SetMode` is GOP-scoped and does not touch DCN. Native resolution is a kernel-side DCN write, not a bootloader trick. |
| Native compositor (aethersafha) | ✅ **Kernel half shipped + iron-proven** — the compositor itself is userland, and the kernel owes it the GPU compositor seam (`#86`–`#89`, 1.55.32), the shader-op descriptor seam (`#92`, 1.56.x), shared memory, the `EFER.NXE`-on-APs fix + cross-CPU TLB shootdown (1.56.35), the channel band (`#97`, 1.56.40) and the pointer band (`#98`, 1.56.42). On **2026-08-03** aethersafha composited two real client windows on archaemenid at `cpus online: 4` — 278 frames, keys delivered, clean quit — and by 2026-08-16 it ran on iron over the channel band, GPU-composited, with a pointer, drag and per-window opacity. Closed-beta MVP still runs without a desktop (agnoshi-as-console via argonaut BOOT_MINIMAL). |
| Pointer input (USB HID mouse) | ✅ **Shipped (1.56.42) + iron-proven.** ⚠ Through 1.56.41 this row read *not possible*: the xHCI enumerator matched HID class `0x03` / subclass `0x01` / **protocol `0x01` only** — the boot *keyboard* — so the compositor had keys but no pointer (it did not block the 2026-08-03 iron run). 1.56.42 binds the boot mouse (protocol `0x02`, including a composite keyboard's mouse interface on the same slot) and hands ring 3 one merged sample per call through `#98 ptrscan`; the wheel followed at 1.56.49. Pointer motion, clicks and a titlebar drag ran on iron by 2026-08-09. |
| Local-IPC channel band (`chan_*` on `#97`) | ✅ **Shipped (1.56.40) + iron-proven 2026-08-07** (1.56.41 kernel: two clients minted, endowed and spawned already connected, 278 frames — the retired TCP path's own numbers, with the transport deleted). ⚠ Through 1.56.40 this row read *in flight*. TCP-on-loopback is retired as the **wrong primitive** for local display IPC (operator ruling 2026-08-03) — a local display protocol has nothing to route, checksum, window or retransmit and no business owning a port. ⚠ Retired on architecture, not on evidence: it did connect un-rigged once `net_src_for` landed. The replacement is a kernel channel band beside `pipe_*` / `shm_*` / `sock_*`, staged as a twelve-bite migration with **no fallback** — not a second transport, no compile-time option, no runtime switch. |
| Per-backend GPT parsing | `gpt.cyr` currently only parses against `blk_active`; partitions on non-active backends aren't reachable. Deferred to next storage-cycle reopening or to a real consumer surfacing demand. |
| ext2/ext4 write paths | ✅ **Shipped (1.33.x)** — block/inode allocator, dirent insertion/removal, file create/truncate/unlink, mkdir/rmdir/rename/ln/symlink, metadata_csum-stamping. W5 demo→base iron burn confirmed at 1.33.1 (`persist.txt` survives reboot on default `mkfs.ext4`). |
| ext4 extent allocation | ✅ **Shipped (1.37.x)** — depth-0 append → depth-0→1 grow → multi-leaf depth-1 sibling split → depth-1→2 index-block grow (the full on-demand grow ladder). Iron-validated 1.37.3. |
| JBD2 crash-safe journaling | ✅ **Shipped + iron-validated (1.38.x)** — probe → log reader → replay-on-mount → in-memory transaction lifecycle → write path (3-barrier sync-checkpoint) → `put_inode` integration → crash-injection smoke + hardening + CSUM_V3 write/replay. AGNOS both consumes Linux-left journals AND produces its own. **Iron-validated on real hardware (1.38.10)**: write-side commit + 100-tx crash stress + mid-cycle power-cut recovery, host `e2fsck -fn` clean throughout. |
| FAT-family (FAT12/16/32 + exFAT) read+write | ✅ **Shipped + iron-validated (1.34.x / 1.40.13)** — partition-aware multi-backend mount, FAT-chain traversal, create/content/delete/truncate/LFN, exFAT allocation bitmap + typed dir-set + up-case table + directory growth, ESP-write guard. **Iron-validated through the shell on real hardware** — mount-namespace routing (1.40.13) makes FAT/exFAT shell verbs reachable while ext2 owns `/`. |
| exec-from-disk (ring-3 programs off the FS) | ✅ **Shipped + iron-validated (1.40.x)** — streaming ELF64 loader (`elf_load_from_file`) → ring-3 execution + exit-code capture → ENOEXEC/E2BIG + subdir/CWD paths → multi-run + argv → process teardown/reaping (1.40.14). **Iron-validated on real Zen** (`/bin/prog2` + `/bin/argv` run in ring 3). Static-only; the interactive shell moving to userland `agnsh` is the 1.41.x shell-separation arc — see row below. |
| Interactive shell (deliberately left the kernel) | ✅ **By design + iron-validated (1.41.x shell-separation arc)** — the full interactive shell is now the userland `agnsh` (agnoshi), loaded from `/bin/agnsh` and run in ring 3; the in-kernel `shell.cyr` shrank to a recovery-only REPL (1149 → 813 LOC at 1.41.9) used as fallback when `/bin/agnsh` is absent. The A1–A4 rubric burn cleared in **June 2026** on archaemenid (boot-to-agnsh ring 3 / recovery fallback / FAT-exFAT write survives power-cycle + fsck clean / no exec-storage-net regression). **Permanent kernel↔userland boundary locked.** |
| HTREE indexed dirs (ext4) | Performance optimization; the linear dirent scan suffices today. Queue when a real consumer needs it. ext4 **symlinks did ship** — `symlink`#63 (fast <60 B inline / slow one-block, `e2fsck`-clean) and its introspection peer `readlink`#70, both ext2-only, landed for the ark/agnova path. |
| Full USB hub / hot-plug | xHCI cmd-path + USB-HID + USB Mass Storage classes shipped (1.30.x → 1.31.3); hub topology + hot-add deferred to plug-and-play cycle. ⚠ agnos binds only the first xHCI controller it finds, and archaemenid has two — a device on the other controller is invisible (found on the 2026-09-08 burn). |
| SMP scheduling (beyond infrastructure) | ✅ **Shipped + iron-validated (1.44.x → 1.46.x)** — AP wake + park at 1.44.18, the SMP locking foundation at 1.46.0, `IF=1` preemptive `agnsh` on iron at 1.46.5. Cross-core scheduling runs on real hardware; archaemenid boots to `smp: cpus online: 4` (the box is 8c/16t and agnos parks APIC id ≥ 4 on purpose). A cross-CPU TLB shootdown IPI landed at 1.56.35. |
| i225-V NIC driver | ✅ **r8169 shipped (1.32.x)** for AMD primary line; i225-V queued for Intel iron post-archaemenid migration (separate hardware line, not an AMD blocker). |
| Optical via USB MS (SCSI MMC profile) | HP external USB Blu-ray on archaemenid derps the NUC at cold boot pre-power-on (firmware quirk). Deferred to plug-and-play cycle; AllInOne internal CD/DVD an alternative path. |
| NTFS / squashfs read | No consumer pressure; deferred. ⚠ Nothing in the distribution path needs squashfs — the ISO pipeline dropped `mksquashfs` along with GRUB and xorriso, so this is a compatibility read only, not a build dependency. |

## Named Subsystems

Every subsystem is a standalone repo at `/home/macro/Repos/{name}/`. Each has its own CLAUDE.md, CHANGELOG, and version. Per-subsystem version numbers intentionally elided below — they drift fast and the registry files are the canonical source.

### ark — Unified Package Manager

User-facing CLI for all package operations. Substantial size/speed gains vs the prior Rust predecessor.

### nous — Package Resolver

Intelligence layer behind ark. Given a package name, determines which source to use.

### sigil — Trust System

System-wide trust and verification. Every binary, package, config, and update is verified through sigil. Ed25519 signing, revocation lists, trust levels.

### kavach — Sandbox Execution

Landlock + seccomp-bpf sandboxing on **Linux targets**; on AGNOS both backends return a structured *not supported* rather than a silent success (native confinement is intended, not built). One required dependency (sigil); samay + ai-hwaccel ride its default `scheduler` feature as optional deps. Substantial size + lifecycle-speed improvements over the prior Rust implementation.

### aegis — System Security Daemon

Unified security and threat protection. Coordinates threat detection, quarantine, and scanning. Cyrius-native; shipped v1.0+ in May 2026. ⚠ It is a library today: its binary prints `aegis ready` and exits rather than running as a daemon (`aegis/src/main.cyr`).

### takumi — Package Build System

Compiles packages from source into `.ark` binary packages via CYML recipes (`.cyml` — CYML allows a TOML header above `---` with markdown below; zugot's recipes use the TOML part only). Recipes live in the `zugot` repo. Cyrius-native — the port reached parity and shipped 1.0+; `rust-old/` is kept as a historical reference, not as the authoritative tree.

### argonaut — Init System

Init system library. Three boot modes: Server, Desktop, Minimal. The BOOT_MINIMAL mode landed agnoshi as a no-deps console service for the closed-beta MVP path.

### bote — MCP Core

MCP message pipeline + host registry. Streamable HTTP via stdlib http_server.

### t-ron — MCP Security

MCP security monitor.

### daimon — Agent Orchestrator

Agent lifecycle, sandboxing, and inter-agent communication. Ships a broad MCP tool catalog (count in the registry).

### hoosh — LLM Gateway

OpenAI-compatible API proxy with a suite of provider backends (count in the registry), caching, rate limiting, hardware acceleration.

### agnoshi — AI Shell

Natural language terminal shell.

### aethersafha — Desktop Compositor

Native compositor with plugin host architecture, Cyrius-native. ⚠ **Not Wayland** — that path is refused by its own ADR; it speaks the sovereign **setu** protocol over two backends: **bhumi** (agnos/host scanout + input seam) and **mehman** (swallow/guest ABI). ⭐ **Iron-proven 2026-08-03** on archaemenid at `cpus online: 4`: two real client windows — setu's `present_probe` and `crab`'s dual-pane file manager — composited on the panel, 278 frames, keys delivered, clean Esc quit, over the CPU blit path. The GPU composite path was iron-proven separately for one opaque surface. Since then, on iron: the clients ride the kernel channel band instead of TCP-on-loopback (2026-08-07), every frame layer is composited on the GPU (2026-08-08), a USB-mouse pointer and a titlebar drag (2026-08-09), and per-window opacity (2026-08-16); the damage, premultiplied-blend, pointer and channel-band items this paragraph listed as open on 2026-08-05 are closed. Still open, per aethersafha's [`state.md`](https://github.com/MacCracken/aethersafha/blob/main/docs/development/state.md): ⚠ the desktop measured **~7–15 fps** on iron on 2026-08-22, and the background clear is **not** the cause (this paragraph had called the full-screen clear the largest frame cost); setu's present protocol has no damage rects; and a client window gets no per-PTY size (no `TIOCSWINSZ` on the `#97` band).

## Security Architecture

⚠ The layers below are the intended model. Several are **Linux-target-only or unbuilt on AGNOS today** — kavach's Landlock/seccomp backends return a structured *not supported* on AGNOS, and there is no namespace-based isolation there — so read per-control status (✅ / 🐧 / 📋 / ⛔) in [security/security-guide.md](security/security-guide.md), not here.

### Defense in Depth

```
+-----------------------------------------------------------+
|  Network: TLS (via sigil), certificate verification        |
+-----------------------------------------------------------+
|  Execution: Landlock + seccomp-bpf (kavach), namespaces    |
+-----------------------------------------------------------+
|  Storage: Encryption, integrity verification               |
+-----------------------------------------------------------+
|  Trust: Ed25519 signing, revocation (sigil)                |
+-----------------------------------------------------------+
|  Audit: Hash-chained log (libro), anomaly detection        |
+-----------------------------------------------------------+
```

### Sandbox Apply Order

1. MAC policy (Landlock)
2. Syscall filtering (seccomp-bpf)
3. Network isolation (namespaces)
4. Audit chain activation

### Package Trust Flow (sigil)

```
Publisher signs package with Ed25519 key
  -> sigil verifies signature against publisher keyring
  -> aegis scans contents
  -> kavach enforces sandbox profile at runtime
  -> libro records to audit chain
```

## Data Flow

### Agent Action Flow

```
User Request -> agnoshi -> daimon -> Agent Process (kavach-sandboxed)
                                |                       |
                           libro audit            hoosh (if LLM needed)
```

### Boot Flow

```
gnoboot (sovereign UEFI bootloader, Path-C handoff)
  -> AGNOS kernel (Cyrius-native, 40+ subsystems, sovereign syscall surface)
  -> kybernet PID 1
  -> argonaut init sequence
  -> daimon agent runtime
  -> hoosh LLM gateway
  -> agnoshi shell / aethersafha desktop
```

⚠ That is the designed chain. What boots on iron today is gnoboot → the kernel → its in-kernel PID 1 (`kernel/user/init.cyr`, which logs as `kybernet:`) → `/bin/agnsh`; nothing yet stages the standalone kybernet, argonaut, daimon or hoosh onto an AGNOS rootfs.

## Technology Stack

| Layer | Technology | Notes |
|-------|-----------|-------|
| Kernel | Cyrius (AGNOS-native) | 40+ subsystems, a small sovereign syscall surface (bands in *Syscalls* above; no BSD socket family, no splice) |
| Compiler | Cyrius (cycc) | self-hosting from 29KB seed; live release + per-repo pins in [`development/state.md`](development/state.md) |
| Bootloader | gnoboot | sovereign UEFI bootloader (PE32+ EFI Application). Replaces GRUB as of v1.30.0 Path-C; selects the largest linear-framebuffer GOP mode at boot rather than inheriting the firmware's; since 0.7.0 bounds-checks the kernel load against the file and loads every `PT_LOAD` segment. |
| User space | Cyrius | All ported subsystems compile with `cyrius build` |
| Host bootstrap | Linux kernel configs | For building cross-compiler on existing host only |
| Build recipes | CYML — `.cyml` (zugot repo) | live counts in the zugot and bazaar READMEs |
| Package format | `.ark` | Built by takumi, installed by ark |

### Pending Cyrius Ports

These subsystems are still pending or in-flight (Rust authoritative):

| Subsystem | Notes |
|-----------|-------|
| bhava | Rust 2.0.0 — emotion/sentiment port pending (Rust authoritative) |
| seema | Edge fleet management — port pending; still `Cargo.toml` + all-Rust `src/`, no `cyrius.cyml` (Rust authoritative) |

Recently shipped (no longer pending): phylax, shakti, hisab, **aegis** (1.0+, Cyrius-native, May 2026), **gnoboot** (sovereign UEFI bootloader, replaced GRUB at the 1.30.0 Path-C cut), **kriya** (coreutils-equivalent multi-tool, M5 grep+find+xargs closeout), **goonj** (acoustics — full Rust→Cyrius rewrite at 2.0.0, 2026-06-30, all 37 modules ported), **naad** (audio synthesis, Cyrius-native), **aethersafha** (Cyrius-native compositor — no longer a scaffold; hosted two client windows on iron 2026-08-03), **takumi** (build system — the Cyrius port reached parity and shipped 1.0+; `cyrius.cyml`, no `Cargo.toml`, and `rust-old/` is a historical reference rather than the authoritative tree). See the v1.0+ [library](applications/libs/README.md) and [binaries & tools](applications/binaries.md) registries for full status.

## Design Decisions

### 1. Sovereign Kernel

AGNOS has its own kernel written in Cyrius — first iron-validated on NUC AMD 2026-05-15 and burned continuously on that box since, most recently 1.57.1 (2026-09-08, burned, **passed**); 1.57.2 → 1.57.10 are built and gated, not burned. No Linux dependency at runtime. Linux kernel configs in this repo are for host bootstrap only.

### 2. Cyrius for Everything

Cyrius is the sovereign systems language — 29KB seed, zero external dependencies, self-hosting compiler. All production subsystems are being ported from Rust to Cyrius. 40+ ports complete (live count in the registry).

### 3. Landlock + seccomp-bpf

Combine unprivileged filesystem sandboxing (Landlock) with syscall filtering (seccomp-bpf). Implemented in kavach (Cyrius-native). ⚠ This is kavach's **Linux-target** design: on AGNOS both backends return a structured *not supported*, and the roadmap rules that AGNOS confinement, when it lands, is capability-scoped and native — never Landlock or seccomp emulation.

### 4. Local-First AI

Prioritize local LLM execution with cloud fallback. Privacy, offline capability, reduced latency. Implemented in hoosh.

### 5. Cryptographic Audit Chain

Immutable, hash-chained, signed audit logs via libro. sigil owns all cryptographic operations.

### 6. Named Subsystems

Major cross-cutting concerns get memorable names for clear identity and discoverability. Each is a standalone repo with its own lifecycle.

### 7. Standalone Repos

Every subsystem is extracted into its own repository. The genesis repo (agnosticos) is meta, narrative, and build wrapper only. This allows independent versioning, per-repo CI, and clear ownership boundaries.

---

## Related Documentation

- [Security Guide](security/security-guide.md)
- [ADR Index](adr/README.md)
