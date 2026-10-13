# AGNOS — Project History & Timeline

> **Status**: Active | **Last Updated**: 2026-09-27

---

## Timeline

| Date | Event |
|------|-------|
| **2026-02-11** | Initial commit. Kernel configuration, Phase 1 (Core OS bootable base), Phase 2 (AI Shell with human oversight), and Phase 5 (Production scaffolding) completed on Day 1 |
| **2026-02-16** | Continued Phase 5 development — production hardening and stabilization |
| **2026-02-22** | Core OS updates and refinement |
| **2026-02-26** | First code audit round — tests, fixes, quality gates |
| **2026-03-04** | Coverage expansion begins |
| **2026-03-05** | **Alpha release** (tag `2026.3.5`) — first tagged release, CalVer versioning adopted |
| **2026-03-06** | Phases 6-7 completed. Code audit work begins in earnest. Marketplace module scaffolded |
| **2026-03-07** | Alpha Docker image published (`ghcr.io/maccracken/agnosticos`). CI/CD pipeline established on GitHub Actions. Multiple audit rounds |
| **2026-03-08** | Release workflow automated (auto-publish instead of draft). Ark package recipes begin |
| **2026-03-09** | Browser builds (Firefox ESR, Chromium), CI integration, database recipe integration |
| **2026-03-10** | Full coverage infrastructure. gRPC, service mesh, OIDC modules. Multiple audit cycles |
| **2026-03-11** | Phase 14 (Edge OS Profile) added to roadmap. Continued audit and repair rounds |
| **2026-03-13** | First ISO build work begins — `build-installer.sh` development |
| **2026-03-14** | aarch64 ISO work — RPi4 ARM64 support |
| **2026-03-15** | RPi4 build fixes. Edge fleet management. Version and release patches |
| **2026-03-16** | Self-hosted runner setup begins for Tier 1 builds. Shared crates published to crates.io |
| **2026-03-17** | Audit completion rounds. Release `2026.3.17` |
| **2026-03-18** | Release `2026.3.18` — major milestone. Photis Nadi migrated from Flutter to Rust native. Consumer app packages updated. Sutra released (v2026.3.18). **LemonSqueezy rejects SecureYeoman** — the first domino |
| **2026-03-19** | Recipe updates and fixes across marketplace |
| **2026-03-20** | Self-hosted runner repaired. ISO build pipeline work continues |
| **2026-03-21** | Build improvements. stiva, nein, t-ron, impetus scaffolded. Multiple ISO build iterations |
| **2026-03-22** | **First successful ISO build** (early morning, after ~9 days of iteration). Abacus desktop calculator released. 266 commits, 298 recipes, 10,800+ tests, ~84.3% coverage |
| **2026-03-24** | Science stack push: 9 crates reach v1.0 in one session (impetus, hisab, bodh, sangha, and others). Agnosys integration ready for consumers |
| **2026-03-25** | Massive session: process refinement, SY migration planning, NPO groundwork |
| **2026-03-28** | AgnosAI benchmarks (4/5 wins vs CrewAI, 2000-4500x faster cached). Release `2026.3.29` |
| **2026-03-31** | **First fully clean release** (`2026.3.31`). All 17 artifacts built successfully — x86_64 ISO (desktop + minimal + edge), aarch64 SD card images (desktop + minimal + edge), userland tarballs, multi-arch Docker container. First release with zero build failures across all architectures. 80 shared crates (45 at v1.0+). 3 new science crates scaffolded (mastishk, rasayan, varna). 336 commits, 19 tagged releases |
| **2026-04-01** | **Monolith dismantled**. agent-runtime, ai-shell, llm-gateway, desktop-environment removed from workspace. 12 standalone repos extracted. 3 crate absorptions (bote 0.91.0, kavach 2.0.0, t-ron 0.90.0). Named subsystems: edge→seema, scheduler→samay. Crypto boundary resolved: sigil owns all AGNOS trust/crypto |
| **2026-04-02** | **Sigil 1.0.0** — first trust crate stable. Bote 0.91.0 — MCP 2025-11-25 spec compliance. **agnosticos.org** domain registered, coming-soon site deployed. 77 shared crates (56 at v1.0+). 95+ marketplace recipes |
| **2026-04-03** | **Cyrius seed** — cyrius-seed 0.1.0 (assembler, 102 tests). **Pure AGNOS desktop boot**: 3.2s, zero external deps. **Full recipe audit**: 109 marketplace recipes. **zugot** decided. **Genesis layer** architecture clarified. **Philosophy** documented. 6 new crates scaffolded (mudra, vinimaya, taal, natya, kshetra, zugot). 28 CLAUDE.md files standardized across ecosystem |
| **2026-04-04** | **Cyrius 1.0** — self-hosting compiler (29KB seed, 42ms bootstrap). Bootstrap loop closed. 44 programs, 58KB kernel (VM, processes, syscalls). Beats GNU on size (10-233x) and speed (wc 2.4x faster). 141 tests, 0 failures |
| **2026-04-05** | **Cyrius ecosystem** — 35 stdlib modules, 8 developer tools, 5 Rust crate rewrites (agnostik, agnosys, kybernet, nous, ark), aarch64 cross-compiler, 38 benchmarks, CI/CD pipelines. 186 tests, dual architecture. Day 3 and counting |
| **2026-05-15** | **Iron boot — MVP spine alive on archaemenid**. After a multi-burn repair ladder, `gnoboot` v0.1.0 (sovereign UEFI loader) hands off to `agnos` 1.30.1, which completes its full init spine on real hardware: GDT/TSS/IDT → APIC + timer → paging → PMM → heap → ACPI/PCI → VFS → initrd → SYSCALL → scheduler arming → idle survival → userland exec → kybernet-launch. Closed-beta gate (cp_fb 0x11 MAGENTA) re-held; four more checkpoints painted past it. The mem-iso block collapsed when a repair deleted the 303-line post-MVP verification test per `uefi-boot-prior-art.md` §6 — turned out to be a detour through a test that wasn't on the boot critical path. |

---

## Development Pace

AGNOS went from initial commit to first bootable ISO in **39 days** (2026-02-11 to 2026-03-22), and from first ISO to first fully clean multi-architecture release in **48 days** (2026-02-11 to 2026-03-31).

From first commit to sovereign self-hosting language with its own kernel in **53 days** (2026-02-11 to 2026-04-04).

The Cyrius language went from nothing to a self-hosting compiler with a kernel in **one day** (2026-04-04), and through major-version cuts (1.0 → 2.0 → 3.0 → 4.0 → 5.0) reaching the v5.x stdlib-foldin cycle (sandhi v5.7.0, vani v5.8.0, niyama v5.9.0) by week 12, then the **REAL TYPE SYSTEM arc** at v5.10.x (24 patches in 2 days). The AGNOS kernel hardened from 143KB (with 14 buffer overflows — "The 143KB Lie") through 220KB (v1.21.0) → 260KB (v1.22.0) → 248KB (v1.26.1), then past 1 MB through the 1.3x–1.4x storage / networking / filesystem / exec-from-disk / shell-separation arcs, and on through the 1.5x graphics / audio / FP arcs to **agnos 1.53.5** — base kernel-internals essentially complete — then straight into the kernel graphics stack: GPU compute on archaemenid's Cezanne shader cores with no amdgpu and no ROCm (1.54.x), display/scanout plus a sovereign ATOM BIOS interpreter and **ACPI S5 self-poweroff** (1.55.x), and shaders → a GPU triangle rasteriser → perspective-correct texturing → native-resolution modeset (1.56.x), arriving 2026-08-03 at **the aethersafha desktop compositing two real client windows on iron**. Live kernel head, binary size and Cyrius pins are in [`docs/development/state.md`](development/state.md). 40+ subsystems ported from Rust to Cyrius, each measured against its Rust predecessor.

The shared crate ecosystem spans a large registry of crates (most at v1.0+ stable) — see [`docs/applications/libs/README.md`](applications/libs/README.md) for the live count — with consumer applications developed in parallel.

---

## Key Milestones

| Milestone | Date | Days from Start |
|-----------|------|----------------|
| First commit | 2026-02-11 | 0 |
| Alpha release | 2026-03-05 | 22 |
| First ISO build | 2026-03-22 | 39 |
| First clean multi-arch release | 2026-03-31 | 48 |
| Monolith dismantled | 2026-04-01 | 49 |
| Cyrius self-hosting | 2026-04-04 | 52 |
| AGNOS kernel (Cyrius) | 2026-04-04 | 52 |
| Cyrius ecosystem (stdlib, tools, crate rewrites) | 2026-04-05 | 53 |
| Kernel v1.21.0 (220KB, 3 hardening passes) | 2026-04-13 | 62 |
| Cyrius 4.0.0 + sovereign boot pipeline | 2026-04-13 | 62 |
| 22+ Cyrius ports, kavach 3.0, abaco 2.0, bote 2.5.1 | 2026-04-14 | 63 |
| Kernel v1.22.0 (260KB), Cyrius 4.8.5-1 (373KB) | 2026-04-14 | 63 |
| Sankoch (compression library) scaffolded | 2026-04-14 | 63 |
| Cyrius 5.0.0 shipped | 2026-04-15 | 63 |
| Cyrius v5.5.x — multi-platform byte-identical (x86_64 Linux, aarch64 Linux on real Pi, Apple Silicon Mach-O, Windows PE32+) | 2026-04-22 | 70 |
| Cyrius v5.6.x — optimization arc (O1 instrumentation + FNV-1a; O2 peephole; linear-scan regalloc default-on); v5.6.45 closeout | 2026-04-25 | 73 |
| **Cyrius v5.7.0 — sandhi-fold** (first stdlib absorption: `lib/sandhi.cyr` 9,649 lines vendored byte-identical from sandhi v1.0.0; sandhi repo enters maintenance mode) | 2026-04-25 | 73 |
| Boot pipeline rebuilt against Cyrius 5.7.21; sigil 2.9.4; ISO `--iso-check` reports 26-of-26 components ready | 2026-04-27 | 75 |
| **AGNOS kernel v1.26.1 (248KB)** — replaces v1.26.0's CI/release-hygiene workaround with a real fix | 2026-04-28 | 76 |
| Cyrius v5.8.x — **66 patches in 4 days** (3-phase: audit closeout, language vocabulary, stdlib foldin sweep with vani-fold at slot 1) | 2026-05-01 → 2026-05-05 | 79–83 |
| **Cyrius v5.9.0 cut — niyama-fold opener** (8th sibling distfile, 5 regex engines, 6,664 lines vendored). cc5 binary at 741,048 B | 2026-05-06 | 84 |
| **Beta rescoped — two-stage**: closed beta (13A + friend-tester cohort) + public beta (adds independent verification + community testing); ADR-008 catch-up records the Cyrius pivot. *(Dates later re-scoped — closed beta → late August 2026 after a founder solo-dogfood month, public beta deferred post-summer; see Target rows below.)* | 2026-05-06 | 84 |
| **Cyrius v5.9.x close — 44 patches over 3 days** (catchup + niyama-fold cycle); pin-lag bands collapse — agnosys/vyakarana/sandhi/cyim/agnostik/owl all roll forward; aegis graduates 0.1.0 → 0.8.2; **darshana** extracted from cyim's TTY layer when chakshu became second consumer | 2026-05-08 | 87 |
| **Cyrius v5.10.x — REAL TYPE SYSTEM arc** opens with per-phase compile-time profiling instrumentation (v5.10.0), pivots at v5.10.5 to type vocabulary (cstring / Result / Option / Tagged) and call-site type checking (Phase 2 at v5.10.24); 24 patches in 2 days. cc5 binary at 783,408 B (+42 KB). Bare-metal + RISC-V rv64 reservation slips to v5.12.x; v5.11.x reserved for TS testing suite + bug sweep | 2026-05-08 → 2026-05-09 | 87–88 |
| **Iron boot — MVP spine alive on archaemenid** (NUC AMD Beelink SER). Path C sovereign UEFI (`gnoboot`) hands off to `agnos` 1.30.1 via the sovereign boot-info struct at RDI; kernel completes its full init spine on real hardware — GDT/TSS/IDT, APIC + timer, paging, PMM, heap, ACPI/PCI enumeration, VFS, initrd, SYSCALL, stack canary, scheduler arming (closed-beta gate cp_fb 0x11 MAGENTA re-held), userland exec test, kybernet-launch site. Four checkpoints past the closed-beta gate. The mem-iso ladder closed when a repair deleted the offending 303-line test block per `uefi-boot-prior-art.md` §6. Closes the "boots-on-iron" front; remaining work to typeable-shell MVP is fb glyph renderer + PS/2-emulation verification for USB keyboards. | 2026-05-15 | 93 |
| **MVP GATE HIT — typeable shell on iron** (agnos 1.30.9). Closed-beta MVP (kernel + kybernet + agnoshi typeable on archaemenid) clears after the xhci silent-absorb arc closed (ending at cyrius v5.11.64's gvar-init-order fix; root cause was an upstream cyrius bug, not silicon) | 2026-05-18 | 96 |
| **Cyrius v6.0.0 cycle-open + two-binary rename ceremony**: `cc5` → `cycc` (production), `cyrc` → `cybs` (bootstrap); v5.11.x closes at **5.11.69** — 70 patches across 11 days, longest minor in Cyrius history | 2026-05-19 | 97 |
| **kashi extracted** (काशि — AGNOS console-font subsystem; freestanding VGA 8x16 + CGA 8x8 glyph cores split from `fb_console.cyr` for parallel-agent development). v1.0+ binaries cohort jumps to 13 with sys-info + terminal-aesthetics burst (mihi/iam/chakshu/bannermanor/darshana/hapi) | 2026-05-20 | 98 |
| **Storage arc (agnos 1.31.x) CLOSED** — all 5 block backends + GPT + ext2/ext4 read landed with iron debuts: NVMe, AHCI/SATA, USB-MS, RAM-disk + VirtIO-blk modern all iron-validated, ext2/ext4 read 1.31.5 + 64BIT 1.31.7 | 2026-05-22 | 100 |
| **r8169 unicast-RX delivery arc CLOSED at 1.32.7** (`missed` collapsed 176 → 0 via RX-ring deepen 16 → 64). **Networking-iron-COMPLETE at 1.32.9** (DHCP `.142` real lease on archaemenid) | 2026-05-25 | 103 |
| **W5 demo→base iron burn PASS** (agnos 1.33.1) — `persist.txt` survives reboot on unmodified default `mkfs.ext4`; **demo→base maturity exit on real NAND**. `fsync` FLUSH-CACHE barrier at 1.33.5 | 2026-05-26 | 104 |
| **FAT-family arc COMPLETE** (1.34.0–1.34.6 across FAT12/16/32 + exFAT read+write + LFN + dir growth + Unicode names + ESP-write guard; `fsck`-clean in QEMU). **1.35.x networking-comms arc COMPLETE** (DNS + ICMP + TCP hardening B0-B4 + NTP + anonymous mmap/munmap + RTC boot clock + DNS cache + arc-close hardening). **1.36.x refactor cycle COMPLETE** (net.cyr split, main.cyr selftest extraction). **1.37.x ext4 extent-allocation arc OPENS** | 2026-05-27 | 105 |
| **ext4 extent allocation iron-validated** (agnos 1.37.3 depth-2 PASS + e2fsck-clean on real NVMe). **1.37.5 arc-close: kashi 0.6.0 vendored into kernel** (retires inline glyph tables). **kashi v1.0.0 API freeze** later same day. **1.38.x JBD2 journaling arc COMPLETE in a single day** — 9 bites: probe → probe-deepen → log reader → replay → lifecycle → write path → integration → crash smoke → hardening + iron-burn audit. AGNOS now both *consumes* Linux-left journals AND *produces* its own; sync-checkpoint with 3 FLUSH-CACHE barriers; `jbd2-crash-smoke.sh` 4/4 e2fsck-clean across SIGKILL points | 2026-05-28 | 106 |
| **JBD2 crash-safe journaling iron-validated** (1.38.10 — CSUM_V3 write-side commit + 100-tx crash stress + mid-cycle power-cut recovery, host `e2fsck -fn` clean throughout). **1.39.x VFS generic-write lift COMPLETE** (FAT/exFAT shell verbs + subdir paths + mount-namespace routing). | 2026-05-30 | 108 |
| **🎯 exec-from-disk iron-validated — base-maturity exec leg closed on real Zen** (`/bin/prog2` + `/bin/argv` run in ring 3, `run: exit 42`/`90`). A single iron boot validated the whole 1.40.x arc — exec (1.40.9) + scheduler-reset fix (1.40.10) + boot-stack relocation (1.40.12) + VFS mount routing (1.40.13): FAT shell verbs pass with ext2 at `/`, clean boot past scheduler to kybernet. **1.40.14 process teardown/reaping.** **1.41.0 shell-separation arc OPENS** (interactive shell → userland `agnoshi`; cyrius-gated `CYRIUS_TARGET_AGNOS` syscall-ABI prereq). | 2026-05-31 | 109 |
| **Shell-separation arc software-complete (agnos 1.41.1 → 1.41.11)** — the interactive shell left the kernel: `kybernet` now execs `/bin/agnsh` in ring 3, and the in-kernel shell shrank to a recovery-only REPL (`shell.cyr` 1149 → 813 LOC at 1.41.9). The `CYRIUS_TARGET_AGNOS` leap (pin 6.0.14 → 6.0.56 at 1.41.4) landed `args_agnos`/`process_agnos` so `agnsh` could run; FS syscall group (getdents/unlink/rename/link/stat) brought the surface to 0-33 (34 calls) at 1.41.3. **Permanent kernel↔userland boundary locked.** QEMU-validated (sweep.sh 7/7, fssys ALL PASS, shsys ALL PASS, agnsh-smoke PASS, check.sh 11/11). **Iron burn PENDING** (first hardware validation of the arc staged with the A1-A4 rubric; not yet booted on real hardware). | 2026-06-04 | 113 |
| **Shell-separation iron-validated — `agnoshi` (ring-3 shell from disk) runs on archaemenid.** The A1–A4 rubric burn cleared: the interactive shell now runs in ring 3, loaded from disk, on real Zen. | 2026-06 | — |
| **1.4x graphics + DOOM in-game on iron** — framebuffer glyph/graphics path and the DOOM engine run in-game on real hardware. | 2026-06 | — |
| **Multi-threading + preemptive scheduling + SMP iron-validated** — preemptive round-robin across cores on real hardware. | 2026-06 | — |
| **1.52.x HDA audio arc — DOOM-WITH-SOUND on iron** — the HDA/Azalia driver drives DOOM audio out the analog front jack on archaemenid. | 2026-07 | — |
| **1.53.x kernel FP/SIMD arc — real f64 in ring 3 on iron** — per-process XMM state; `f64`/SIMD DSP validated on real Zen. **agnos 1.53.5 — base kernel-internals essentially complete.** | 2026-07 | — |
| **Kernel GPU-compute arc (1.54.x) — opened and closed in four days.** From the first write to archaemenid's AMD Cezanne iGPU (PSP GPCOM ring-create) through PSP firmware load, GFXHUB GMC setup and the first PM4 packet + doorbell, to hand-assembled gfx90c shaders running integer tiled matmul (1.54.29) and full-precision f64 matmul (1.54.31) with **no amdgpu and no ROCm** — the f64 result **bit-identical to rosnet's CPU math including rounding** (1.54.32). Exposed to ring 3 at `#82` / `#83 gpu_dispatch_f64` | 2026-07-11 → 2026-07-14 | 150–153 |
| **★ AGNOS POWERS ITSELF OFF — ACPI S5, iron-validated** (1.55.26): the full S5 sequence from the agnsh prompt, power LED out on archaemenid, `_S5_` read live off the AMI DSDT. Same day the **sovereign ATOM BIOS interpreter is PROVEN ON IRON** (1.55.24) — VBIOS from the ACPI VFCT table; encoder and transmitter runs emit *exactly* the amdgpu oracle's write sequences | 2026-07-19 | 158 |
| **First hardware 2D on agnos** — CP-DMA copy, fill and true strided blit all verified on iron in one boot (1.55.30), the blit's inter-row padding untouched. The next day the **GPU compositor seam `#86`–`#89` is IRON-PROVEN: a whole mock compositor frame with zero per-pixel CPU work** (1.55.32), closing the display arc as the 1.56.x shader arc opens | 2026-07-21 → 2026-07-22 | 160–161 |
| **agnos has a GPU triangle rasteriser** (1.56.17) — 167 hand-authored gfx90c instructions, **20 of 20 cases byte-identical** to the CPU reference on iron with every negative control firing. The rung ladder then closed barycentric RGBA, texturing, bilinear, depth test, and **perspective-correct texturing** (1.56.31) — the last proven on a corpus where the affine and perspective references differ at 731 of 1541 covered pixels | 2026-07-25 → 2026-07-29 | 164–168 |
| **⭐ THE DESKTOP COMPOSITES TWO REAL CLIENT WINDOWS ON IRON.** archaemenid boots to `smp: cpus online: 4` and the aethersafha compositor hosts `present_probe` and **crab's dual-pane file manager** as windows on the panel — 278 frames, keys delivered to the client, clean Esc quit. The blocker was one line (agnos 1.56.35): the AP trampoline set `EFER \|= 0x100` (LME only) where the BSP sets `0x900` (LME\|NXE), so bit 63 of every NX paging entry was RESERVED and each W^X data page faulted on an AP. Cross-CPU TLB shootdown landed in the same cut | 2026-08-03 | 173 |
| **Native 2560x1440 scanout + a hardware-panned boot console — RELEASED and BURNED PASS** (1.56.36 / 1.56.37 / 1.56.38): the scaler in DSCL bypass, no banded first frame, and a modeset latch that releases itself at clean shutdown. `Timer ticks before sched` 28 (800x600) → 149 (native, software scroll) → **11** (native + pan) | 2026-08-03 → 2026-08-04 | 173–174 |
| **Kernel head agnos 1.56.40 — OPEN, not burned**: the local-IPC **channel band** (`chan_*`), replacing TCP-on-loopback, retired 2026-08-03 as the *wrong primitive* for local display IPC — ⚠ retired as wrong, **not** as a thing that never worked. Decisions closed the same day: `#96` = `fork`, `#97` = `chan_op`, no codename. Live versions, sizes and pins → [`state.md`](development/state.md) | 2026-08-05 | 175 |
| **The channel band meets silicon — all three subjects PASS** (the 1.56.41 kernel carrying 1.56.40's `chan_*` band): the band, puka as the real terminal, and the shell answering typed keys, on iron. setu 0.8.0 had removed the TCP dial from its agnos client the day before (its listen/accept side kept TCP until 0.8.4, the same day, when TCP left setu entirely) | 2026-08-07 | 177 |
| **⛔ PS/2 deleted from the kernel; the pointer lands on iron** (1.56.42): the USB mouse binds, a GPU cursor, click-to-focus and a titlebar drag with a real release edge. Every desktop frame layer had moved to the GPU on 2026-08-08 (an aethersafha change on the 1.56.41 kernel). On 2026-08-10 four compositor launches run in one boot and the covered-console `klug` spill is verified byte-exact against an independent ring dump | 2026-08-08 → 2026-08-10 | 178–180 |
| **HDMI audio shelved after five burns** — every source-side observable says playing and there is no sound; the leg needs an oracle that observes the wire. Same day three HID input-path defects are fixed (1.56.43) and a burn shows the keyboard path unregressed — the mouse half and halt recovery have not run on iron — and Backspace plus file timestamps on the real NVMe volume pass on iron | 2026-08-11 | 181 |
| **The sovereign shader pipeline on iron** (1.56.44): ring 3 issues `#92` op 0x06 and a translucent client surface composites for hundreds of frames — per-window opacity is real on iron (aethersafha 0.16.2). The premultiplied rounding tie stays unsettled | 2026-08-16 | 186 |
| **The desktop's iron defect list is empty** — the background flicker fixed at its producer (aethersafha 0.16.18) after a broad-then-narrow switch experiment. Three days later the first frame-cost measurement reads **~7–15 fps on iron, and the clear is not why** | 2026-08-19 → 2026-08-22 | 189–192 |
| **Consumer syscalls and a hardening sweep** (1.56.47 → 1.56.60): `#99 proclist`, `#100 icmp_echo_ex`, `#101 readdir_at`, `#102 lstat`, `#96 fork`, `#103 statfs`, `#104 mountlist`; the size-multiply overflow class and a toolchain pin `build.sh` enforces; a ring-3 port-I/O hole closed (the TSS I/O-map base at the wrong offset); kernel-log timestamps; a recovery shell that powers off. Burned PASS 2026-08-30 (1.56.53, real DHCP / DNS / HTTPS) and 2026-09-03 (1.56.60); `#96 fork` and the recovery shell's power verbs have not been exercised on iron | 2026-08-24 → 2026-09-03 | 194–204 |
| **Cyrius v6.5.x closes at 6.5.73; 6.6.0 opens v6.6.x** — `Result` / `Option` / `Either` become value-form, the minor's one breaking change, shipped with the ecosystem migrated at source | 2026-09-06 | 207 |
| **The first kernel built by cyrius 6.6 runs on iron** (1.57.0), then 1.57.1's backlog closeout burns PASS on four real cores — the **last iron burn to date** | 2026-09-07 → 2026-09-08 | 208–209 |
| **AGNOS ships a proportional TrueType face** a client can open — Liberation Sans, embedded unmodified in the kernel at `/fonts/default.ttf` for rekha to rasterize (1.57.2) | 2026-09-13 | 214 |
| **Kernel head agnos 1.57.10 — built and gated, NOT burned.** The 1.57.x repairs minor: per-process kernel stacks, the blocking model (every wait blocks only its caller), a `kill` that acts, sockets with owners, blocking pipe and channel I/O, and **VT-d translation turned on for the first time** (QEMU only). Cyrius **6.6.6** shipped 2026-09-20. Live versions and pins → [`state.md`](development/state.md) | 2026-09-24 → 2026-09-26 | 225–227 |
| **Target: Closed beta cut** (after a ~July founder solo-dogfood month) | **late August 2026** — passed with no recorded opening; no revised date on record | — |
| **Target: Public beta** | **deferred post-summer 2026** | — |
| **Target: GA** | **late fall / early winter 2026** | — |

---

## Compiler Binary Naming

The Cyrius compiler binary has been renamed four times over the language's evolution. Each rename created drift across scripts, CI, install paths, and docs — which motivates the final rename to `cycc` at v6.0 to end the treadmill by decoupling binary name from language version.

| Binary | Cyrius era | Notes |
|--------|------------|-------|
| `cc` | v1.x | Initial self-hosting compiler (2026-04-04) |
| `cc2` | v2.x | First rename (v2.0, ~2026-04-08) |
| `cc3` | v3.x and v4.x | Stayed across both major versions (v3.0 shipped ~2026-04-09; persisted through v4.8.x on 2026-04-14) |
| `cc5` | v5.x | `cc3` → `cc5` at v5.0.0 (2026-04-15) — **cc4 was never shipped**; that binary name/version was skipped |
| `cycc` | v6.x (current) | `cc5` → `cycc` ("Cyrius Computer Compiler") at v6.0.0 ceremony on 2026-05-19. Bootstrap `cyrc` → `cybs` ("Cyrius Bootstrap") in the same ceremony. **Names are now permanent** — no `cycc7` at v7.0.0; the cc3 → cc5 (v5.0.0) → cycc (v6.0.0) sequence was the LAST name-change penalty paid |

Four renames total across four language-major transitions. The `cc5` → `cycc` event at v6.0 (landed 2026-05-19) is the one-and-done cleanup that future major versions inherit without further renaming. Back-compat symlinks `cc5 → cycc` + `cyrc → cybs` shipped through the v6.0.x window and were dropped at v6.1.1 (2026-06-08); `~/.cyrius/bin/` now carries `cycc` and `cybs` only.

---

*Last Updated: 2026-09-27*
