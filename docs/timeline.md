# AGNOS — Project Timeline

> **Status**: Active | **Last Updated**: 2026-09-27
>
> All dates verified from git commit history (`git log --format="%ai"`).
> Times are Pacific (PT).

---

## Phase 1 — The Rust Era (Feb 11 – Apr 2)

| Date | Day | Event | Source |
|------|-----|-------|--------|
| **2026-02-11** | 0 | Initial commit. Phase 1 (Core OS), Phase 2 (AI Shell), Phase 5 (Production) scaffolded in one afternoon | `agnosticos 6d4d043` |
| **2026-02-16** | 5 | Production hardening begins | `agnosticos f985c40` |
| **2026-02-22** | 11 | Core OS refinement | `agnosticos 1c1022b` |
| **2026-02-26** | 15 | First code audit round — tests and quality gates | `agnosticos 3b4ab8f` |
| **2026-03-02** | 19 | Critical fixes cycle begins | `agnosticos c022343` |
| **2026-03-05** | 22 | **Alpha release** (`2026.3.5`) — first tagged release, CalVer adopted | `agnosticos 9a12aa9` |
| **2026-03-06** | 23 | Phases 6-7 complete. Marketplace scaffolded | `agnosticos c005d9f` |
| **2026-03-07** | 24 | Alpha Docker image published. CI/CD pipeline on GitHub Actions | `agnosticos 6a74094` |
| **2026-03-08** | 25 | Release workflow automated (auto-publish). Ark recipes begin | `agnosticos 55dda3f` |
| **2026-03-09** | 26 | Browser builds (Firefox ESR, Chromium). Database recipe integration | `agnosticos 325f33e` |
| **2026-03-10** | 27 | Full coverage infrastructure. gRPC, OIDC, service mesh modules | `agnosticos a85cefe` |
| **2026-03-13** | 30 | First ISO build work — `build-installer.sh` development | `agnosticos` |
| **2026-03-14** | 31 | aarch64 ISO work — RPi4 ARM64 support | `agnosticos` |
| **2026-03-16** | 33 | Self-hosted runner setup. Shared crates published to crates.io | `agnosticos` |
| **2026-03-18** | 35 | Release `2026.3.18`. **LemonSqueezy rejects SecureYeoman** — the first domino | `agnosticos` |
| **2026-03-22** | 39 | **First successful ISO build** — 266 commits, 298 recipes, 10,800+ tests | `agnosticos` |
| **2026-03-24** | 41 | Science stack push: 9 crates reach v1.0 in one session | `agnosticos` |
| **2026-03-28** | 45 | AgnosAI benchmarks (4/5 wins vs CrewAI, 2000-4500x faster cached) | `agnosticos` |
| **2026-03-31** | 48 | **First fully clean multi-arch release** (`2026.3.31`). Zero build failures across all architectures. 80 shared crates | `agnosticos` |
| **2026-04-01** | 49 | **Monolith dismantled**. 12 repos extracted. Crypto boundary resolved: sigil owns all trust/crypto | `agnosticos` |
| **2026-04-02** | 50 | **Sigil 1.0.0** stable. **agnosticos.org** domain registered | `agnosticos` |

---

## Phase 2 — The Cyrius Era (Apr 3 – present)

### Week 1: Language Birth (Apr 3–6)

| Date | Day | Time | Event | Source |
|------|-----|------|-------|--------|
| **2026-04-03** | 51 | 03:06 | Cyrius repo scaffolded | `cyrius 09a568b` |
| | | 03:50 | Phase 0 — seed binary hardened | `cyrius acf6e48` |
| | | 06:06 | Stage 1 — first compiler output | `cyrius cf05f4e` |
| | | 06:12 | "cyrius the lang" — name chosen | `cyrius 6620d54` |
| **2026-04-04** | 52 | 03:50 | **Cyrius v1.0** — self-hosting compiler. Variables, arithmetic, if/else, while, factorial | `cyrius 49f46a5` |
| | | 04:01 | v2 complete | `cyrius 6aeff25` |
| | | 04:03 | "bye bye rust" | `cyrius 345598e` |
| | | 12:11 | "language works ohhhh" | `cyrius fd38672` |
| | | 13:12 | Benchmark beater — Cyrius programs outperform GNU coreutils | `cyrius 469189d` |
| | | 22:16 | Kernel VGA output | `cyrius 6ab3ef1` |
| | | 22:54 | 64-bit mode achieved | `cyrius ee69ab5` |
| | | 23:16 | **"kernel solid"** — Cyrius kernel boots | `cyrius 7fd4d6b` |
| | | 23:34 | Page tables, keyboard, memory management | `cyrius 2b19664` |
| | | 23:40 | "kernal ready" | `cyrius 55f101d` |
| **2026-04-05** | 53 | 00:05 | **aarch64 cross-compiler** — "aaarch baby aarch" | `cyrius a93f4ac` |
| | | 01:07 | Enums with auto-increment + explicit values | `cyrius 9dd7ef5` |
| | | 02:55 | AGNOS kernel repo scaffolded | `agnos` first commit |
| | | 03:38 | aarch64 confirmed working | `cyrius 8cc9649` |
| **2026-04-06** | 54 | 01:55 | **Cyrius 1.0.0 tagged** | `cyrius tag 1.0.0` |
| | | 02:20 | AGNOS kernel: context switching | `agnos` |
| | | 02:44 | Kernel: syscalls | `agnos` |
| | | 03:12 | Ring 3 userland | `agnos` |
| | | 04:11 | VFS | `agnos` |
| | | 05:26 | PCI bus driver | `agnos` |
| | | 05:44 | IP/UDP network stack | `agnos` |
| | | 06:46 | "main work done" | `agnos` |
| | | 07:31 | **AGNOS kernel v1.0** | `agnos` |

### Week 2: Ecosystem Expansion (Apr 7–9)

| Date | Day | Event | Source |
|------|-----|-------|--------|
| **2026-04-07** | 55 | Cyrius v1.7.7 — stdlib growth, dep system maturing | `cyrius` |
| **2026-04-08** | 56 | **Cyrius 2.0.0** tagged. **cyrius-doom Sprint 1 begins** — black screen at 21:31, BSP by 21:35, textures by 22:24, sprites by 22:38 | `cyrius tag 2.0.0`, `cyrius-doom` |
| **2026-04-09** | 57 | **Cyrius 3.0.0** tagged. DOOM Sprint 1 wraps — v0.17.0, 129KB, Episode 1 renderable. Patra included in stdlib | `cyrius tag 3.0.0` |

### Week 3: Ports, Hardening, DOOM Plays (Apr 10–14)

| Date | Day | Event | Source |
|------|-----|-------|--------|
| **2026-04-10** | 58 | Patra dep integration. Crate ports accelerating | `cyrius` |
| **2026-04-13** | 61 | **Cyrius 4.0.0** tagged. **DOOM Sprint 2** — gameplay end-to-end, P(-1) security audit, 5 CVE-class findings fixed. AGNOS kernel hardening: 6 buffer overflow fixes, security phases 1-3 | `cyrius tag 4.0.0`, `cyrius-doom`, `agnos` |
| | | Kernel v1.21.0 (220KB). kybernet 1.0.1 ported. Boot pipeline active | |
| **2026-04-14** | 62 | **Cyrius 4.8.5-1** tagged. 22+ Cyrius ports complete. kavach 3.0.0, abaco 2.0.0, bote 2.5.1 shipped. Kernel v1.22.0 (260KB). **Independent verification (external env)** — bootstrap verified, kernel booted, benchmarks green. **Sankoch named** — compression library identified as last git blocker | `cyrius tag 4.8.5-1`, `agnos`, audit conversation |

### Week 4: Sankoch, 5.0, Releases (Apr 15)

| Date | Day | Event | Source |
|------|-----|-------|--------|
| **2026-04-15** | 63 | 01:30 — **Sankoch scaffolded**. 01:57 — compression extracted. 02:29 — "v1 ready?". 03:33 — 3 bugs found. By morning — **5,762 assertions passing, beats C zlib** | `sankoch` commits |
| | | **hisab 2.2.0** — Cyrius port complete (linalg.cyr in stdlib unblocked it) | `hisab` |
| | | **shravan 2.3.2** — modernization + security audit. MDCT 5.35x faster | `shravan` |
| | | **Cyrius 4.9.x–4.10.x** — linalg.cyr, sankoch stdlib integration, CYML parser | `cyrius` tags |
| | | 12:31 — **Cyrius 5.0.0 tagged and merged** — cc5 IR, CFG, cyrius.cyml, patra v1.0, sankoch in stdlib | `cyrius tag 5.0.0` |
| | | 12:45 — Roadmap cleanup. **5.0 shipped.** | `cyrius` |

---

## Summary

| Metric | Value | Days from Start |
|--------|-------|-----------------|
| First commit | 2026-02-11 | 0 |
| Alpha release | 2026-03-05 | 22 |
| First ISO build | 2026-03-22 | 39 |
| First clean multi-arch release | 2026-03-31 | 48 |
| Monolith dismantled | 2026-04-01 | 49 |
| LemonSqueezy rejection (first domino) | 2026-03-18 | 35 |
| Cyrius scaffolded | 2026-04-03 | 51 |
| Cyrius self-hosting (v1) | 2026-04-04 | 52 |
| "kernel solid" | 2026-04-04 23:16 PT | 52 |
| Cyrius 1.0.0 tagged | 2026-04-06 | 54 |
| AGNOS kernel v1.0 | 2026-04-06 | 54 |
| Cyrius 2.0.0 | 2026-04-08 | 56 |
| DOOM Sprint 1 (renders) | 2026-04-08–09 | 56–57 |
| Cyrius 3.0.0 | 2026-04-09 | 57 |
| Cyrius 4.0.0 | 2026-04-13 | 61 |
| DOOM Sprint 2 (plays, hardened) | 2026-04-13 | 61 |
| Kernel v1.22.0 (260KB) | 2026-04-14 | 62 |
| Cyrius 4.8.5-1 (373KB, 42 stdlib modules) | 2026-04-14 | 62 |
| Independent verification (external env) | 2026-04-14 | 62 |
| Sankoch v1.0 (beats C zlib) | 2026-04-15 | 63 |
| hisab 2.2.0, shravan 2.3.2 | 2026-04-15 | 63 |
| **Cyrius 5.0.0 shipped** | **2026-04-15 12:31 PT** | **63** |

### Cyrius Stdlib Foldin Cycle (Apr 22 – May 6+)

| Date | Day | Event | Source |
|------|-----|-------|--------|
| **2026-04-22** | 70 | Cyrius v5.5.x — **multi-platform byte-identical** (x86_64 Linux, aarch64 Linux on real Pi, Apple Silicon Mach-O, Windows PE32+) | `cyrius` repo |
| **2026-04-25** | 73 | Cyrius v5.6.45 — optimization arc closeout (O1 instrumentation + FNV-1a; O2 peephole; linear-scan regalloc default-on) | `cyrius` repo |
| **2026-04-25** | 73 | **Cyrius v5.7.0 — sandhi-fold** (first stdlib absorption: `lib/sandhi.cyr` 9,649 lines vendored byte-identical from sandhi v1.0.0; sandhi enters maintenance mode per ADR 0002) | `agnosticos/CHANGELOG.md`, `sandhi` repo ADR 0002 |
| **2026-04-27** | 75 | Boot pipeline rebuilt against Cyrius 5.7.21; sigil 2.9.4; ISO `--iso-check` reports 26-of-26 components ready | `agnosticos/CHANGELOG.md` |
| **2026-04-28** | 76 | **AGNOS kernel v1.26.1 (248KB)** — replaces v1.26.0's CI/release-hygiene workaround with a real fix | `agnosticos/CHANGELOG.md` |
| **2026-05-01 → 2026-05-05** | 79–83 | **Cyrius v5.8.x — 66 patches in 4 days** (3-phase: audit closeout, language vocabulary, stdlib foldin sweep with vani-fold at slot 1) | `cyrius` repo |
| **2026-05-06** | 84 | **Cyrius v5.9.0 cut — niyama-fold opener** (8th sibling distfile, 5 regex engines: bre/re2/pcre/fuzzy/vim, 6,664 lines vendored). cc5 binary at 741,048 B | `cyrius` repo |
| **2026-05-06** | 84 | **Beta rescoped — two-stage**: closed beta (13A + friend-tester cohort) + public beta (adds verification + community testing). *(Later rescoped again — see the note below the timeline: closed beta late August 2026 after a ~July founder solo-dogfood month; public beta deferred post-summer; GA late fall/early winter 2026.)* | agnosticos CHANGELOG |
| **2026-05-06** | 84 | **ADR-008 catch-up** — Cyrius pivot (2026-04-04) formally recorded; ADR-001 marked partially superseded (language only) | `agnosticos/docs/adr/adr-008-cyrius-as-sovereign-systems-language.md` |
| **2026-05-08** | 87 | **Cyrius v5.9.x cycle close at 5.9.43** — 44 patches over 3 days. Catchup + niyama-fold cycle. Pin-lag bands collapse: agnosys / vyakarana / sandhi / cyim / agnostik / owl roll forward. **aegis graduates** 0.1.0 → 0.8.2. **darshana** (TTY/raw-mode primitives, दर्शन — *viewing*) extracted from cyim's `src/tty.cyr` when chakshu became second consumer | `cyrius/CHANGELOG.md` |
| **2026-05-08 → 2026-05-09** | 87–88 | **Cyrius v5.10.x — REAL TYPE SYSTEM arc** opens. v5.10.0 ships per-phase compile-time profiling instrumentation; v5.10.5 pivots to type vocabulary (cstring / Result / Option / Tagged); v5.10.24 lands Phase 2 call-site type checking. 24 patches in 2 days. cc5 binary at **783,408 B** (+42 KB from instrumentation + type machinery) | `cyrius/CHANGELOG.md` |
| **2026-05-09** | 88 | **Cycle reservations slip** — bare-metal + RISC-V rv64 → v5.12.x (was v5.10 → v5.11 → v5.12); v5.11.x reserved for TS testing suite + agnosys-agent-surfaced bug sweep | agnosticos state notes |
| **2026-05-09** | 88 | **Doc tree re-org** — `docs/development/doc-health.md` → `docs/doc-health.md` (whole-tree scope); `docs/architecture/kernel-layers.md` inlined into `docs/architecture.md`; `docs/development/applications/` → `docs/development/planning/` (73 files / 108 cross-refs); `docs/os/README.md` deleted (redundant with architecture § Named Subsystems) | `agnosticos/CHANGELOG.md` |

### Iron-Boot + Arc-Series Surge (May 13 – May 28)

| Date | Day | Event | Source |
|------|-----|-------|--------|
| **2026-05-13** | 92 | **GRUB MB2-EFI Path-A blocker** — `grub_relocator64_efi_boot` faults under OVMF 2024+ strict W^X (writes to its own .text patching `_efi_start`'s register-state immediates). Pivot to Path-C sovereign UEFI bootloader (`gnoboot`); cyrius gains UEFI-application emit support across v5.11.43–55 | agnosticos Path-C bring-up notes |
| **2026-05-15** | 94 | **Iron MVP spine alive on archaemenid** (NUC AMD Beelink SER). Path-C sovereign UEFI handoff → `agnos` 1.30.1 → full init spine reaches userland-exec test + kybernet-launch site. Four checkpoints past the closed-beta gate. Closes the boots-on-iron front; remaining work to typeable shell = fb glyph renderer + USB-keyboard via xHCI/HID | iron-validated on real hardware |
| **2026-05-18** | 97 | **MVP GATE HIT — typeable shell on iron** (agnos 1.30.9). Closed-beta MVP (kernel + kybernet + agnoshi typeable on archaemenid) clears after the xhci silent-absorb arc closed (ending at cyrius v5.11.64's gvar-init-order fix) | iron-validated on real hardware |
| **2026-05-19** | 98 | **Cyrius v6.0.0 cycle-open + two-binary rename ceremony**: `cc5` → `cycc` (production), `cyrc` → `cybs` (bootstrap). v5.11.x closes at 5.11.69 — **70 patches across 11 days** (longest minor in Cyrius history): stdlib annotation arc (1,010 fns), consumer-issue closeout, ELF section-header fix arc, gvar-init-order fix, heap-map full reorg | `cyrius/CHANGELOG.md` v6.0.0 / v5.11.69 |
| **2026-05-20** | 99 | **kashi extracted** (काशि — "shining"; AGNOS console-font subsystem) — split from `fb_console.cyr`'s inline VGA 8x16 + CGA 8x8 tables for parallel-agent development. v1.0+ binaries cohort jumps to 13 (also: `mihi`, `iam`, `bannermanor`, `chakshu` jump to 1.0) | `kashi` repo M0, `project_kashi_parallel_split` memory |
| **2026-05-22** | 101 | **Storage arc (agnos 1.31.x) CLOSED** — all block backends + GPT + ext2/ext4 read landed across 1.31.0 → 1.31.7 with iron debuts: NVMe, AHCI/SATA, USB-MS, RAM-disk + VirtIO-blk modern all iron-validated, GPT Phase 1-3, ext2/ext4 read Phase 1-4. **Networking arc (1.32.0) OPENS** | `agnos/CHANGELOG.md` `[1.31.x]` |
| **2026-05-25** | 104 | **r8169 unicast-RX delivery arc CLOSED** at 1.32.7 — five letter-bites narrowed the unicast-RX defect from "L2 filter" to "RX ring delivery capacity"; bite-5 deepened ring 16 → 64, `missed` collapsed 176 → 0. **Networking-iron-COMPLETE at 1.32.9** (DHCP lease `.142` verified on archaemenid). **ext2/ext4 WRITE arc OPENS** (1.33.x) | `agnos/CHANGELOG.md` `[1.32.7]`/`[1.32.9]` |
| **2026-05-26** | 105 | **W5 demo→base iron burn PASS** (agnos 1.33.1) — `persist.txt` survives reboot on unmodified default `mkfs.ext4` partition; demo→base maturity exit confirmed on real NAND. `fsync` FLUSH-CACHE barrier added at 1.33.5. **FAT-family arc OPENS** (1.34.x) | `agnos/CHANGELOG.md` `[1.33.x]` |
| **2026-05-27** | 106 | **FAT-family arc COMPLETE** across 1.34.0–1.34.6 (FAT12/16/32 + exFAT read+write + LFN + dir growth + Unicode names + ESP-write guard, all `fsck`-clean in QEMU). **1.35.x networking-comms arc COMPLETE** (DNS + ICMP + TCP-hardening B0-B4 + NTP + mmap/munmap syscalls + RTC boot clock + DNS cache + arc-close hardening). **1.36.x refactor ops COMPLETE** (net.cyr split into 8 protocol files, main.cyr selftest extraction). **1.37.0 ext4 extent-allocation arc OPENS** | `agnos/CHANGELOG.md` `[1.34.x]` / `[1.35.x]` / `[1.36.x]` / `[1.37.0]` |
| **2026-05-28** | 107 | **ext4 extent allocation iron-validated** at 1.37.3 (depth-2 PASS + e2fsck-clean on real NVMe). **1.37.5 arc-close: kashi 0.6.0 vendored into kernel** (retires inline VGA 8x16 tables in `fb_console.cyr`; consumes via `[deps.kashi]`). **kashi v1.0.0 (API freeze)** later same day. **1.38.x JBD2 journaling arc COMPLETE in a single day** — 9 bites (probe / probe-deepen / log reader / replay / lifecycle / write path / integration / crash smoke / hardening + iron-burn audit). AGNOS now both *consumes* Linux-left journals AND *produces* its own; sync-checkpoint with 3 FLUSH-CACHE barriers; `jbd2-crash-smoke.sh` 4/4 e2fsck-clean across SIGKILL points | `agnos/CHANGELOG.md` `[1.37.x]` / `[1.38.x]`, `kashi/CHANGELOG.md` `[1.0.0]` |
| **2026-05-30** | 108 | **JBD2 crash-safe journaling iron-validated** (1.38.10 — CSUM_V3 write-side commit + 100-tx crash stress + mid-cycle power-cut recovery; host `e2fsck -fn` clean throughout). **1.39.x VFS generic-write lift COMPLETE** — FAT/exFAT shell verbs + subdir paths, capped by 1.40.13 mount-namespace routing | `agnos/CHANGELOG.md` `[1.38.10]`–`[1.39.9]` |
| **2026-05-31** | 109 | **🎯 exec-from-disk iron-validated — base-maturity exec leg closed on real Zen** (`/bin/prog2` + `/bin/argv` ring-3, exit 42/90). A single iron boot validated the whole 1.40.x arc — exec + scheduler-reset fix + boot-stack relocation + VFS mount routing; FAT shell verbs pass with ext2 at `/`, clean boot past scheduler to kybernet. **1.40.14 process teardown/reaping.** **1.41.0 shell-separation arc OPENS** (interactive shell → userland `agnoshi`; cyrius-gated `CYRIUS_TARGET_AGNOS` ABI prereq) | `agnos/CHANGELOG.md` `[1.40.x]`–`[1.41.0]` |
| **2026-05-31 → 2026-06-04** | 109–113 | **Shell-separation arc software-complete** across 1.41.1 → 1.41.11. FS syscalls land (getdents 29 / unlink 30 / rename 31 / link 32 / stat 33 — surface now 0–33) at **1.41.3**; **boot-to-`agnsh`** ring-3 userland shell at **1.41.4** (cyrius pin leaps 6.0.14 → 6.0.56 to land `lib/args_agnos.cyr` + `lib/process_agnos.cyr` under `CYRIUS_TARGET_AGNOS`); syscall-ingress hardening at **1.41.5**; kernel shell shrinks to a recovery-only REPL at **1.41.9** (`shell.cyr` 1149 → 813 LOC, −336); **software-complete at 1.41.11** (build 1,070,720 B). QEMU-green (`sweep.sh` 7/7, `fssys` ALL PASS, `shsys` ALL PASS, `agnsh-smoke` PASS, `check.sh` 11/11); iron burn staged (A1–A4 rubric) | `agnos/CHANGELOG.md` `[1.41.1]`–`[1.41.11]` |

### Base Kernel-Internals Completion (June – July 7)

| Date | Event | Source |
|------|-------|--------|
| **June** | **Shell-separation iron-validated** — `agnoshi`, the ring-3 interactive shell loaded from disk, boots and runs typeable on archaemenid (real Zen). The kernel REPL is recovery-only; the shell is now a userland program | `agnos/CHANGELOG.md` |
| **June** | **Graphics + DOOM in-game on iron** — framebuffer scanout + DOOM playable end-to-end on real hardware | `agnos/CHANGELOG.md`, `cyrius-doom` |
| **June** | **Multi-threading + preemptive scheduling + SMP** iron-validated — round-robin preemption and multi-core bring-up on archaemenid | `agnos/CHANGELOG.md` |
| **1.52.x** | **HDA audio iron-validated — DOOM-WITH-SOUND out the analog front jack** on real Zen (HDA/Azalia driver + ring-3 `snd_*` band + cyrius `vani` agnos backend; LAPIC-calibration fix at 1.52.8) | `agnos/CHANGELOG.md` `[1.52.x]` |
| **1.53.x** | **Kernel FP/SIMD iron-validated — real `f64` in ring 3** on real Zen (per-proc XMM state, lazy `#NM` save/restore; `fpex` → 84, `naadex` sine-oscillator DSP → 88) | `agnos/CHANGELOG.md` `[1.53.x]` |
| **2026-07-07** | **Kernel head agnos 1.53.5.** Base kernel-internals essentially complete: boot → disk → FS-crash-safe → exec-from-disk → networking → ring-3 shell → graphics/DOOM → SMP → audio → FP/SIMD, all iron-validated on archaemenid. Toolchain: **Cyrius 6.4.16**, **gnoboot 0.6.0** (sovereign UEFI). Focus shifts to a ~July founder solo-dogfood month ahead of late-August closed beta | `agnos`, `cyrius`, `gnoboot` |

### Kernel Graphics Stack → The Desktop on Iron (July 11 – August 5)

| Date | Event | Source |
|------|-------|--------|
| **2026-07-11 → 2026-07-14** | **Kernel GPU-compute arc (1.54.x) opens and closes in four days.** From the first write to archaemenid's AMD Cezanne iGPU (PSP GPCOM ring-create) through PSP firmware load, GFXHUB GMC setup, the first PM4 packet + doorbell, to the first hand-assembled gfx90c compute shader (1.54.14). The crowns: integer tiled matmul (1.54.29) and full-precision f64 matmul (1.54.31) running on the shader cores with **no amdgpu and no ROCm** — the f64 result **bit-identical to rosnet's CPU math including rounding**, both sides summing k-ascending (1.54.32). Exposed to ring 3 at `#82` and `#83 gpu_dispatch_f64` — attn11's path to the GPU | `agnos/CHANGELOG.md` `[1.54.0]`–`[1.54.33]` |
| **2026-07-14** | **Kernel DISPLAY arc opens (1.55.0)** — read-only DCN 2.1 live-pipe probe, then agnos's first DCN write (the scanout flip, 1.55.3), vblank pacing (1.55.4), and a double-buffered present loop paced by the display's own clock → **tear-free full-screen apps with no app change** (1.55.6) | `agnos/CHANGELOG.md` `[1.55.0]`–`[1.55.6]` |
| **2026-07-19** | **★ AGNOS POWERS ITSELF OFF — ACPI S5, iron-validated** (1.55.26): `poweroff` at the agnsh prompt runs the full S5 sequence and the power LED goes out on archaemenid, with `_S5_` read live off the AMI DSDT. Same day the **sovereign ATOM BIOS interpreter is PROVEN ON IRON** (1.55.24) — VBIOS acquired from the ACPI VFCT table, and the encoder and transmitter runs emit *exactly* the amdgpu oracle's write sequences (5 writes, then 17). `reboot` resets the machine on iron at 1.55.25 | `agnos/CHANGELOG.md` `[1.55.24]`–`[1.55.26]` |
| **2026-07-21** | **First hardware 2D on agnos — CP-DMA copy, fill and true strided blit all VERIFIED on iron in one boot** (1.55.30), via a 7-dword PM4 `DMA_DATA` on the proven MEC compute ring; the blit's inter-row padding stayed untouched, proving real per-row stride addressing rather than a linear copy. Ring-3 consumer `/bin/gpufill` → exit 95 (1.55.31) | `agnos/CHANGELOG.md` `[1.55.30]`/`[1.55.31]` |
| **2026-07-22** | **The GPU compositor seam `#86`–`#89` — IRON-PROVEN: a whole mock compositor frame with ZERO per-pixel CPU work** (1.55.32, `/bin/gpublit` → exit 95). The display arc closes; **1.56.0 opens the SHADER arc** (alpha, translucency, text) | `agnos/CHANGELOG.md` `[1.55.32]`/`[1.56.0]` |
| **2026-07-25** | **agnos has a GPU triangle rasteriser** (1.56.17) — two blobs, 167 hand-authored gfx90c instructions, one lane per pixel, no sort / no LDS / no cross-lane op. **20 of 20 cases byte-identical** to the CPU reference on iron, every negative control N1–N8 firing | `agnos/CHANGELOG.md` `[1.56.17]` |
| **2026-07-29** | **Perspective-correct texturing iron-closed** (1.56.31, exit 95) — byte-identical to the perspective reference at every covered pixel on a corpus where the affine and perspective references provably differ at **731 of 1541** covered pixels, so the per-pixel divide demonstrably ran. The rung ladder had already closed barycentric RGBA interpolation (1.56.20), texturing with WRAP/FULLCOV/COLMAJOR (1.56.21–1.56.23), bilinear (1.56.29) and depth clear + depth test (1.56.30), each on iron | `agnos/CHANGELOG.md` `[1.56.20]`–`[1.56.31]` |
| **2026-08-03** | **⭐ THE DESKTOP COMPOSITES TWO REAL CLIENT WINDOWS ON IRON.** archaemenid boots to `smp: cpus online: 4` and the aethersafha compositor hosts `present_probe` and **crab's dual-pane file manager** as windows on the panel — **278 frames**, keys delivered to the client, clean Esc quit. The blocker was one line (agnos 1.56.35): the AP trampoline set `EFER \|= 0x100` (LME only) where the BSP sets `0x900` (LME\|NXE), so with NXE clear bit 63 of a paging entry is RESERVED and every W^X data page and user stack faulted on an AP (`kernel/arch/x86_64/smp.cyr:542`). Cross-CPU TLB shootdown landed in the same cut and rode the burn clean | `agnos/CHANGELOG.md` `[1.56.35]`, `docs/development/state.md` |
| **2026-08-03 → 2026-08-04** | **Native 2560x1440 scanout + a hardware-panned boot console — RELEASED and BURNED PASS** (1.56.36 / 1.56.37 / 1.56.38): the scaler in DSCL bypass, no banded first frame (the panel is held until the geometry is verified, then the boot log is repainted from klug), and a modeset latch that releases itself at clean shutdown. Instrument: `Timer ticks before sched` **28** (800x600) → **149** (native, software scroll) → **11** (native + pan) ⇒ native and the pan are ONE change, not two — taking the console native *without* the pan is the 149 | `agnos/CHANGELOG.md` `[1.56.36]`–`[1.56.38]`, `docs/development/state.md` |
| **2026-08-05** | **Kernel head agnos 1.56.40 — OPEN and NOT burned**: the local-IPC **channel band** (`chan_*`), replacing TCP-on-loopback, which was retired 2026-08-03 as the *wrong primitive* for local display IPC. ⚠ Retired as **wrong**, not as a thing that never worked. Its last two decisions closed the same day: `#96` = `fork`, `#97` = `chan_op`, and the band gets **no codename**. Live kernel head, binary size, syscall surface and Cyrius pins → [`state.md`](development/state.md) | `agnos/CHANGELOG.md` `[1.56.39]`/`[1.56.40]`, `docs/development/state.md` |

### The Desktop Moves to the GPU → Hardening and the Cyrius 6.6 Line (August 7 – September 26)

| Date | Event | Source |
|------|-------|--------|
| **2026-08-07** | **The channel band meets silicon — all three subjects PASS** (the 1.56.41 kernel carrying 1.56.40's `chan_*` band): the band, puka as the real terminal, and the shell answering typed keys, on iron. setu 0.8.0 had removed the TCP dial from its agnos client the day before (its listen/accept side kept TCP until 0.8.4, the same day, when TCP left setu entirely) | `agnos/CHANGELOG.md` `[1.56.40]`/`[1.56.41]`, `docs/development/iron-nuc-zen-log.md#tracker-15641-desktop-band` |
| **2026-08-08 → 2026-08-10** | **⛔ PS/2 deleted from the kernel; the pointer lands on iron** (1.56.42): the USB mouse binds on every boot-mouse interface, a GPU cursor, click-to-focus and a titlebar drag with a real release edge; every desktop frame layer moved to the GPU on 2026-08-08 (an aethersafha change on the 1.56.41 kernel). 2026-08-10: four compositor launches in one boot after the relaunch fix, and the covered-console `klug` spill verified **byte-exact** against an independent ring dump | `agnos/CHANGELOG.md` `[1.56.42]`, `iron-nuc-zen-log.md#tracker-15642-relaunch` |
| **2026-08-11** | **HDMI audio SHELVED after five burns** — every source-side observable says playing and there is no sound; the leg needs an oracle that observes the wire. Same day three HID input-path defects are fixed (1.56.43) and a burn shows the keyboard path unregressed — the mouse half and halt recovery have not run on iron — and Backspace plus file timestamps on the real NVMe volume pass on iron | `agnos/CHANGELOG.md` `[1.56.43]`, `docs/development/iron-log.md` |
| **2026-08-16** | **The sovereign shader pipeline on iron** (1.56.44): ring 3 issues `#92` op 0x06 and a translucent client surface composites for hundreds of frames — per-window opacity is real on iron (aethersafha 0.16.2). ⚠ The premultiplied rounding tie stays unsettled | `agnos/CHANGELOG.md` `[1.56.44]`, `aethersafha/CHANGELOG.md` `[0.16.2]` |
| **2026-08-19 → 2026-08-22** | **The desktop's iron defect list is empty** — the background flicker fixed at its producer (aethersafha 0.16.18) after a broad `--fullclear` / narrow `--clearbg` experiment pair. Then the first frame-cost measurement on iron: **~7–15 fps, and the clear is not why** — a separate problem that was always there | `iron-nuc-zen-log.md#tracker-desktop-b9-result`, `aethersafha/docs/development/state.md` |
| **2026-08-24 → 2026-09-03** | **Consumer syscalls and a hardening sweep** (1.56.47 → 1.56.60): `#99 proclist`, `#100 icmp_echo_ex`, `#101 readdir_at`, `#102 lstat`, `#96 fork` (1.56.54, corrected 1.56.55), `#103 statfs`, `#104 mountlist`; the size-multiply overflow class and a toolchain pin `build.sh` now enforces (1.56.51); a ring-3 port-I/O hole closed — the TSS I/O-map base sat at the wrong offset (1.56.52); kernel-log timestamps (1.56.58); a recovery shell that powers off (1.56.60). `#105 blkstats` minted and withdrawn. **Burned PASS** 2026-08-30 (1.56.53, the seven-cut validation burn — a real DHCP lease, DNS and an HTTPS fetch) and 2026-09-03 (1.56.60); ⚠ `#96 fork` and the recovery shell's power verbs have not been exercised on iron | `agnos/CHANGELOG.md` `[1.56.47]`–`[1.56.60]`, `iron-nuc-zen-log.md#tracker-iron-v1` / `#tracker-iron-v2` |
| **2026-09-06** | **Cyrius v6.5.x closes at 6.5.73; 6.6.0 opens v6.6.x** — `Result` / `Option` / `Either` become value-form, the minor's one breaking change, shipped with the ecosystem migrated at source | `cyrius/CHANGELOG.md` `[6.5.73]`/`[6.6.0]` |
| **2026-09-07 → 2026-09-08** | **The first kernel built by cyrius 6.6 runs on iron** (1.57.0, zero faults), then **1.57.1**'s backlog closeout burns PASS — `tlm` and `mlist` exit 95 on four real cores. ⚠ The **last iron burn to date** | `agnos/CHANGELOG.md` `[1.57.0]`/`[1.57.1]`, `iron-nuc-zen-log.md#tracker-iron-v3` / `#tracker-iron-v4` |
| **2026-09-13** | **AGNOS ships a proportional TrueType face** a client can open — Liberation Sans, embedded unmodified in the kernel at `/fonts/default.ttf` for rekha to rasterize (1.57.2), filed by crab | `agnos/CHANGELOG.md` `[1.57.2]` |
| **2026-09-20** | **Cyrius 6.6.6** — the last patch of v6.6.x's reserved repair window | `cyrius/CHANGELOG.md` `[6.6.6]` |
| **2026-09-24 → 2026-09-26** | **Kernel head agnos 1.57.10 — built and gated, NOT burned.** 1.57.x is the arising-issue-repairs minor: per-process kernel stacks (1.57.6); the blocking model — every wait blocks only its caller — with a `kill` that acts, sockets with owners, `#106 sock_peer` and `#107 spawn_limits` (1.57.7); blocking pipe and channel I/O (1.57.8 / 1.57.9); **VT-d translation turned on for the first time**, in QEMU (1.57.10). Live head → [`state.md`](development/state.md) | `agnos/CHANGELOG.md` `[1.57.6]`–`[1.57.10]` |
| **2026-09-28** | **The closed beta goes undated.** It slipped past early June and then late August 2026, and per the operator there is still a great deal to do before AGNOS is usable even for the founder's own daily work — the beta waits on that, not on a calendar date | `docs/development/roadmap.md` § *Ship-milestone ladder* |

### Pace

- **Rust era** (51 days): initial commit → monolith → ISO → multi-arch release → dismantled
- **Cyrius era week 1** (12 days, Apr 3–15): nothing → self-hosting compiler → kernel → DOOM → 28 ports → compression that beats C → 5.0 shipped
- **Cyrius era weeks 2–4** (Apr 16 – May 6, 21 days): multi-platform byte-identical → optimization arc → 3 stdlib fold-ins (sandhi/vani/niyama) → kernel hardening to 248KB → 30+ ports
- **Cyrius era week 5** (May 7–9, 3 days): v5.9.x close (44 patches; consumer-rollup catchup; aegis graduates; darshana extracted) → v5.10.x REAL TYPE SYSTEM arc opens (24 patches in 2 days)
- **Iron-boot week 6** (May 13–18, 5 days): GRUB MB2-EFI W^X blocker → Path-C sovereign UEFI ladder → MVP spine → typeable-shell MVP gate (xhci silent-absorb arc closed by cyrius v5.11.64)
- **Arc-series surge** (May 19–31, 13 days): cyrius v6.0.0 + binary renames → kashi extracted → storage (1.31.x) → networking (1.32.x) → ext2/4 WRITE (1.33.x) → FAT-family (1.34.x) → networking-comms (1.35.x) → refactor ops (1.36.x) → extent allocation (1.37.x) + kashi fold-in + kashi v1.0 → JBD2 journaling (1.38.x) → VFS generic-write lift (1.39.x) → exec-from-disk (1.40.x) → shell-separation arc opens (1.41.0). **11 minor arcs in 13 days**; the FS-crash-safe (1.37–1.39) + exec-from-disk (1.40.x) base-maturity legs both iron-validated on real Zen.
- **Shell-separation arc** (May 31 – Jun 4, ~5 days): 1.41.1 → 1.41.11 — FS syscalls (surface 0–33) → boot-to-`agnsh` ring-3 userland shell (cyrius pin 6.0.14 → 6.0.56) → ingress hardening → kernel shell shrunk to recovery-only REPL (`shell.cyr` 1149 → 813 LOC). Software-complete + QEMU-validated, then **iron-validated in June** — the ring-3 shell (`agnoshi`) boots typeable on real hardware.
- **Base kernel-internals completion** (June – Jul 7): shell-separation iron-validated → graphics + DOOM in-game on iron → multi-threading + preemptive scheduling + SMP → HDA audio (**DOOM-with-sound out the analog front jack**, 1.52.x) → kernel FP/SIMD (real `f64` in ring 3, 1.53.x). **Kernel head 1.53.5; base internals essentially complete**, all major legs iron-validated on archaemenid.
- **Kernel graphics stack → the desktop** (Jul 11 – Aug 5, 26 days): GPU compute (1.54.x — first write to the Cezanne iGPU → f64 matmul rosnet-bit-correct → ring-3 `#82`/`#83`) → display/scanout + the sovereign ATOM interpreter + **ACPI S5 self-poweroff** (1.55.x) → first hardware 2D via CP-DMA → the GPU compositor seam → shaders → a **GPU triangle rasteriser** → perspective-correct texturing → native-resolution modeset (1.56.x). **⭐ 2026-08-03: the aethersafha desktop composites two real client windows on iron at `cpus online: 4`** — 278 frames, keys delivered, clean Esc quit; native 2560x1440 scanout + a hardware-panned boot console burn PASS at 1.56.36/37/38. ⚠ agnos 1.56.40 (the local-IPC channel band) was open and unburned on 2026-08-05; it burned PASS on 2026-08-07, carried by the 1.56.41 kernel.
- **The desktop moves to the GPU → hardening and the Cyrius 6.6 line** (Aug 7 – Sep 26, 51 days): the channel band on silicon → PS/2 deleted, the pointer, every frame layer on the GPU (1.56.42) → HDMI audio shelved → the sovereign shader pipeline and per-window opacity on iron (1.56.44) → the desktop's iron defect list emptied, then ~7–15 fps measured → consumer syscalls (`#96` fork, `#99`–`#104`) and a hardening sweep (1.56.47–1.56.60) → the Cyrius 6.6 line (1.57.0) → the 1.57.x repairs minor: per-process kernel stacks, the blocking model, VT-d. **Iron-validated through 1.57.1 (2026-09-08); 1.57.2 → 1.57.10 are unburned.**
- **Language versions in 176 days** (Cyrius v1.0 on 2026-04-04 → 2026-09-27): 1.0 → 2.0 → 3.0 → 4.0 → 5.0 → 5.5.x → 5.6.x → 5.7.x → 5.8.x → 5.9.x → 5.10.x → 5.11.x → 6.0.x (closed 6.0.91) → 6.1.x → 6.2.x → 6.3.x → 6.4.x (closed 6.4.86) → 6.5.x (generated-code quality, closed 6.5.73) → **6.6.x** (value-form `Result` + language ergonomics, active) — live release in [`state.md`](development/state.md)

---

*All timestamps from `git log` or canonical CHANGELOG/state.md sources. No estimates, no approximations. Last updated 2026-09-27.*
