# CIS Benchmarks — AGNOS Mapping

**Last Updated**: 2026-09-27

> **This is a mapping, not an attestation.** AGNOS holds **no CIS certification**, and none is claimed. No compliance percentage has been measured against the AGNOS kernel. AGNOS is **Pre-Beta** (agnos 1.56.40 / cyrius 6.5.7 / gnoboot 0.6.1 at the time of writing; re-verified 2026-09-27 against agnos 1.57.10 — built and gated in QEMU, not burned — / cyrius 6.6.6 / gnoboot 0.7.2). Consistent with the compliance table in [`SECURITY.md`](../../SECURITY.md), CIS is 📋 **Planned**.

The CIS Distribution Independent Linux Benchmark is written for a **Linux distribution**. Every one of its six sections presupposes a subsystem AGNOS does not have: uid/gid separation and `/etc/passwd`, PAM, systemd, `/etc/fstab` mount options, sysctl, auditd, rsyslog, TCP wrappers, iptables/nftables, and an apt/dnf-shaped package manager. AGNOS has none of them, and per [`docs/development/roadmap.md`](../development/roadmap.md) it is not going to grow them — confinement, when it lands, is "**capability-scoped and native** — never Landlock, seccomp or `unshare` ABI emulation."

So most of this document's predecessor was not *stale*. It was **categorically inapplicable**, and it marked ~55 controls ✅ against subsystems that do not exist. That is an audit liability, and it has been removed rather than updated.

## Status vocabulary

| Marker | Meaning |
|--------|---------|
| ✅ **Shipped** | Verified in live source at the cited `path:line` |
| 🐧 **Linux-target only** | Real, but only on a Linux build target or the Linux **host-bootstrap** kernel. **Not an AGNOS control** |
| 📋 **Aspirational** | Intended; no mechanism exists today |
| ❌ **Absent** | No mechanism, and no CIS-shaped analogue |

## Two different targets — do not conflate them

1. **The AGNOS kernel** — a single Cyrius binary. There are **no loadable kernel modules** and no LSM framework. This is what "AGNOS" means below, and it is what CIS is being mapped against.
2. **Linux host-bootstrap kernels** — `kernel/{6.6-lts,6.x-stable,7.0-devel}/configs/agnos_defconfig` and `kernel/configs/edge-*.config`. These build the *host* used to stage AGNOS. Their `CONFIG_*` rows are real, but they say nothing about AGNOS. They are marked 🐧 throughout.

⚠️ Neither of those Linux configs is consumed by a live pipeline. Their consumers repo-wide are `scripts/archive-pre-cyrius/build-kernel.sh:160` and `.github/archive-pre-cyrius/selfhost-build.yml:865-866` — both **archived Rust-era** artifacts — plus the operator-run `kernel/scripts/download-kernels.sh:143-154`, which copies `agnos_defconfig` into a downloaded tree and which nothing invokes. The Cyrius boot pipeline (`scripts/src/boot.cyr`) has no kernel-config step.

---

## Control-family mapping

Verdict key: **(a)** maps directly · **(b)** maps to a different AGNOS mechanism · **(c)** does not apply.

### 1.1 Filesystem Configuration

| CIS intent | Verdict | AGNOS reality | Evidence |
|---|---|---|---|
| `/etc/fstab` mount options (`noexec`, `nosuid`, `nodev`, separate `/tmp`) | **(c)** ❌ | There is no `/etc/fstab` and no mount-option enforcement. `mount#11` and `umount#24` are **no-op stubs that return 0** | `agnos/kernel/core/syscall.cyr:9668,9670` (agnos 1.57.10) |
| The *intent* behind `noexec` — data must not be executable | **(b)** ✅ | Enforced **per page, not per mount**: W^X maps every non-`PF_X` PT_LOAD segment, every user stack, and all `sys_mmap` anonymous memory NX (bit 63); since 1.56.51 executable segments are mapped read-only, and since 1.57.2 (built and gated, not burned) an RWX segment is refused | `agnos/kernel/core/proc.cyr:2386-2445` · `core/elf.cyr:281-298,321-322,371` |
| — caveat on the above | ⚠️ | NX only *means* anything if `EFER.NXE` is set, and **it is set — but only the APs assert it.** Each secondary CPU's trampoline writes `EFER = LME\|NXE` explicitly (`smp.cyr:1072-1083`, after an `-smp 4` reserved-bit `#PF` proved the omission). The **BSP** on the shipping ELF64/UEFI path writes EFER nowhere — the branch that would (`boot_shim.cyr:103-105`) is under `#ifndef ELF64_KERNEL` and `agnos/scripts/build.sh:308` defines that macro, so it is dead code; the live path's header says "CR0/CR3/CR4/EFER — UEFI configured them." NXE is therefore **inherited from firmware on the BSP and never verified**. It has in fact been observed set — EFER read back `0xd00` on two independent live boots, once as an NX stack fault actually firing — so W^X **is enforced today**. The gap is the missing assertion: firmware that left NXE clear would void W^X silently, with no boot-time error | `agnos/kernel/arch/x86_64/smp.cyr:1072-1083` · `boot_shim.cyr:103-105,221` · `ring3.cyr:146-155` · `syscall_hw.cyr:378-385` |
| Sticky bit on `/tmp` (1777) | **(c)** ❌ | Requires a permission model. ext2 preserves `i_mode` on disk, but the kernel enforces nothing against it — there is no uid to check and no `chmod` syscall | `agnos/kernel/core/ext2.cyr:295-307` · `core/syscall.cyr:9667` |
| Disable USB storage | **(c)** ❌ **inverted** | USB mass storage is a **compiled-in kernel driver AGNOS can boot from**, not a removable module. There is no config knob to disable it | `agnos/kernel/core/block.cyr:32,150-156,185,251` · `arch/x86_64/usb/xhci.cyr` |
| Disable FireWire / Thunderbolt / automount | 🐧 | `CONFIG_FIREWIRE=n`, `CONFIG_THUNDERBOLT=n`, `CONFIG_AUTOMOUNT=n` are present in the **host** defconfig. AGNOS has no such drivers to begin with | `kernel/6.6-lts/configs/agnos_defconfig:107,111,101` |

⚠️ The predecessor claimed `CONFIG_TMPFS_XATTR=n` and `CONFIG_HUGETLB_PAGE=n`. **Both symbols are absent from every host defconfig.**

### 1.2 / 2.2 Legacy and special-purpose services

**(c) Does not apply — and "compliant" here would be vacuous.** CIS asks you to disable xinetd/rsh/telnet/ftp/tftp/avahi/CUPS/LDAP/NFS/Samba/Dovecot/SNMP. AGNOS ships **no package set at all**, so none of them is present — not because a control removed them, but because nothing installed them. Marking that ✅ tells a reviewer a decision was made where none was.

Two rows are worth stating plainly because AGNOS's actual behaviour **contradicts** the CIS intent:

- **Telnet.** AGNOS ships **agora**, a telnet-served BBS whose default listener is port **2323** (`agora/src/main.cyr:36`). CIS 1.2.3 says no telnet; an AGNOS operator running agora is deliberately not complying.
- **HTTP server.** A running AGNOS system serves HTTP from its own daemons — daimon on port 8090 (`daimon/src/config.cyr:11`; it asks for `127.0.0.1` since daimon 2.3.0 and bound `0.0.0.0` before), hoosh on `127.0.0.1:8088` (`hoosh/src/main.cyr:82,88`), and bote's four MCP transports across `127.0.0.1:8390-8393` when explicitly launched — one `bote` binary since bote 3.4.2, the transport picked by subcommand in `fn main` (`bote/src/main.cyr:98`): `bote http` on 8390 (`main.cyr:184-191`), `bote bridge` on 8391 (`:200-210`), `bote streamable` on 8392 (`:135-156`) and `bote ws` on 8393 (`:157-165`). `bote-streamable` / `bote-ws` survive only as argv[0] aliases (symlinks to `bote`, matched in `bote/src/main_common.cyr:146-149`); a bare `bote` runs stdio and opens no port (`main_common.cyr:152`). ⚠️ Those `127.0.0.1` addresses are what the services ask for; on AGNOS the cyrius 6.6.6 stdlib they are built on drops the address and listens on the NIC. cyrius 6.6.7 fixed the stdlib, but none of the three has been rebuilt on it as of 2026-09-28 (see [`SECURITY.md`](../../SECURITY.md) § *Network Security*). CIS 2.2.9 says no HTTP server.

The honest control here is not a disable-list; it is the enumerated listening set and its caveats in [`SECURITY.md`](../../SECURITY.md) § *Network Security*. ⚠️ **There is no firewall** — no netfilter, no nftables, no packet-filter hook anywhere in the AGNOS kernel — so a port is closed only because no program opened it.

### 2.1 Time Synchronization

**(b) Maps to a different mechanism.** No chrony, no systemd-timesyncd, no systemd at all — PID 1 today is agnos's in-kernel init in the kybernet role (`agnos/kernel/user/init.cyr`, which logs as `kybernet:`) — the standalone kybernet binary is not staged on AGNOS. AGNOS instead has a **kernel-native SNTP client**: `agnos/kernel/core/net_ntp.cyr`.

✅ Shipped, with the scope its own header declares: RFC 4330 simple unicast, one UDP/123 query, "no offset calc, no drift discipline" (`net_ntp.cyr:6-7`). ⚠️ **It is unauthenticated** — a grep for auth/NTS/key material in `net_ntp.cyr` returns nothing. CIS's underlying intent (time comes from a *trusted* source) is **not** met; only the sync itself is.

### 3.1 / 3.2 Network parameters (sysctl)

**(c) Does not apply to AGNOS.** There is no sysctl subsystem, no `/proc/sys`, and no `/etc/sysctl.d` in the AGNOS kernel.

🐧 `config/sysctl/99-agnos-hardening.conf` exists in this repo and is a correct CIS 3.1/3.2 fragment **for a Linux host**. ⚠️ Nothing deploys it today: its only writers are `scripts/archive-pre-cyrius/build-installer.sh:552-554` and `build-sdcard.sh:553-555` (archived bash), applied by `config/init/agnos-init.sh:163-164` under a **systemd unit** (`config/systemd/system/agnos-init.service:10`). AGNOS runs kybernet, not systemd. The predecessor's claim that it is "deployed during OS installation" is false for **agnova**, the installer that ships.

What the AGNOS network stack does have, verified, is narrower and worth naming for what it is rather than mapping onto a sysctl row:

| Control | Status | Evidence |
|---|---|---|
| Ingress length hardening — a claimed IPv4 total-length is clamped to bytes actually received | ✅ Shipped | `agnos/kernel/core/net_ingress.cyr:323-334` |
| Receive checksums (IPv4 header, ICMP, UDP, TCP) verified and IPv4 fragments refused, each with a drop counter | ✅ Shipped at 1.56.52 — before it, no receive checksum was checked at any layer | `agnos/kernel/core/net_ingress.cyr:365-426` · counters `core/net.cyr:115-120` |
| TCP ISN unpredictability (RDRAND-seeded, timer-LCG XOR fallback) | ✅ Shipped | `agnos/kernel/core/net_tcp.cyr:899-906` |
| Wire martian filter — 127/8 source or destination, a ≥ 224 source, or our own address as a source is dropped at the wire | ⚠️ Since 1.57.7, built and gated, **not burned**; counted (`net_drop_martian`), not logged | `agnos/kernel/core/net.cyr:121-123` |
| SYN cookies, rp_filter, redirect/source-route policy | ❌ Absent | No equivalent knob or code path exists |

### 3.3 TCP Wrappers

**(c) ❌ Does not apply.** No libwrap, no `/etc/hosts.allow`, no `/etc/hosts.deny`, and no packet filter to substitute for them.

### 3.4 Uncommon network protocols (DCCP / SCTP / RDS / TIPC)

**(a) Maps — and this is the one family where AGNOS's answer is structurally stronger than the control asks.**

CIS asks you to disable four protocol modules. AGNOS has **no protocol-family selector to disable**. The network band is a fixed set of purpose-built syscalls over one kernel TCP/UDP/ICMP stack — `sock_connect#47`, `sock_send#48`, `sock_recv#49`, `sock_close#50`, `udp_bind#51`…`icmp_echo#55`, `sock_listen#56`, `sock_accept#57`, and since 1.56.48 / 1.57.7 `icmp_echo_ex#100` and `sock_peer#106` — none of which takes an address-family or protocol argument (`agnos/kernel/core/syscall.cyr:42-43`; 1.57.7's `sock_listen` address *class*, any vs loopback, is not a family selector). There is no generic BSD socket family, no `socket()` over arbitrary domains, no `splice`, no `AF_ALG`. This is the same property that makes the kernel structurally immune to CVE-2026-31431; see [`SECURITY.md`](../../SECURITY.md) § *Notable Hardening*.

⚠️ State it precisely. **AGNOS does have sockets** — a curated, slot-indexed band. What it does not have is a family multiplexer. "AGNOS has no sockets" is false and must never be written.

🐧 On the host-bootstrap side, `CONFIG_SCTP=n`, `CONFIG_RDS=n` and `CONFIG_TIPC=n` are pinned (`kernel/6.6-lts/configs/agnos_defconfig:114-116`). ⚠️ **DCCP is not pinned** — the predecessor cited `CONFIG_MPTCP=n`/`CONFIG_NDISC=n`, which are both absent from the file and are the wrong symbols regardless (upstream is `CONFIG_IP_DCCP`).

### 4.1 System accounting (auditd)

**(b) Maps to a different mechanism — with a different tamper model.** There is no auditd, no `/etc/audit/rules.d`, and no audit rule language. AGNOS's audit substrate is **libro** (2.10.3 on 2026-09-27): append-only, SHA-256 hash-linked entries with opt-in per-entry Ed25519 signing, Merkle inclusion and RFC 9162 consistency proofs (`libro/src/chain.cyr:157-166` append, `:349-366` verify).

✅ Shipped as a library. Be precise about what that buys:

- The chain makes tampering **detectable, not preventable**. `libro/src/file_store.cyr` is `flock`-guarded JSONL; on AGNOS every process is uid 0 with no MAC, so any process can truncate or rewrite it. Detection requires an out-of-band copy of the head hash.
- 📋 There is **no system-wide audit enable switch** and no remote log shipper (see [`SECURITY.md`](../../SECURITY.md) § *System Installation*). Today's writers are daimon and argonaut, not the kernel.
- ⚠️ The predecessor's `agnos-audit` package and `systemctl enable auditd` do not exist. Neither does the row claiming gnoboot passes `audit=1` / `audit_backlog_limit=8192` — those are **Linux** kernel command-line parameters, and gnoboot boots the AGNOS kernel.

### 4.2 Logging

**(b) Maps to a different mechanism, and only partly.** No rsyslog, no syslog daemon, no `/etc/rsyslog.conf`. Userland logging is **sakshi** (2.5.5 on 2026-09-27); the kernel logs to the console. ❌ There is no log rotation, no remote forwarding, and no central collector — the CIS 4.2 intent is **not** met.

### 5.1 PAM

**(c) ❌ Structurally impossible.** There is no PAM on AGNOS: no `/etc/pam.d`, no `/etc/security/pwquality.conf`, no `faillock`, no `/etc/shadow`.

🐧 `zugot/base/linux-pam.cyml` is a Linux userland recipe; `aegis/src/pam.cyr` is Linux-host-only. **shakti** does real PAM auth via `unix_chkpwd(8)` — but shakti is **pre-1.0 (0.7.2)**, contains **zero** `CYRIUS_TARGET_AGNOS` guards, and hardcodes Linux `SYS_CAPGET`/`SYS_CAPSET` numbers. It is a Linux tool. There is no kernel seam behind it on AGNOS, and since agnos 1.56.56, applying the 2026-05-12 identity-model ruling, the kernel records it as N/A there by architecture.

### 5.2 / 6.2 User accounts, UID/GID hygiene

**(c) ❌ Structurally impossible, and CIS 6.2.1 is inverted.**

`getuid#15` returns **0 unconditionally, for every process** — `agnos/kernel/core/syscall.cyr:9667`, inline comment `# getuid (root)`. There are no users, no UIDs, no GIDs, no `/etc/passwd`, no password aging and no login. CIS 6.2.1 asks that root be the *only* UID 0 account; on AGNOS **every process is UID 0**. The kernel says so itself in `core/power.cyr:410-411`: *"`getuid` is hardcoded 0 and there is no uid model, so a uid check would be a gate that is always open."*

📋 An identity and authorization model is a planning track ([`docs/development/planning/identity-and-authorization-model.md`](../development/planning/identity-and-authorization-model.md)), not a shipped control. Since agnos 1.56.56 the kernel records that track's ruling as permanent (`core/power.cyr:412-418`): there will be no per-process uid — identity is meant to come from sigil, t-ron and avatara overlays, and authorization from kavach capabilities. CIS's uid rows stay inapplicable by design, not by delay.

### 6.1 System file permissions

**(c) ❌ Does not apply.** `/etc/passwd`, `/etc/shadow`, `/etc/group` and `/etc/gshadow` do not exist, so there are no modes to audit. There is no `chmod` syscall and no permission enforcement path.

---

## AGNOS-native controls with no CIS row

These are the controls AGNOS actually has. CIS has no place to record them, which is itself part of why the mapping is thin.

| Control | Status | Evidence |
|---|---|---|
| Ring 0 / ring 3 separation; kernel pages mapped supervisor-only (`0x83`), user pages U/S (`0x87` data, `0x85` read-only text) | ✅ Shipped | `agnos/kernel/arch/x86_64/paging.cyr:7` · `core/proc.cyr:2366-2445` |
| Per-process address space — a fresh PML4/PDPT/PD per process | ✅ Shipped | `agnos/kernel/core/proc.cyr:1419-1568` |
| User-pointer validation — a page-table walk requiring present + U/S under the live CR3 | ✅ Shipped at 1.56.52 (through 1.56.51 a VA-window check that admitted kernel pages) | `agnos/kernel/core/syscall.cyr:354-378,509-533` |
| W^X — NX on data segments, user stacks, and anonymous `mmap`; text read-only since 1.56.51; RWX segments refused since 1.57.2 (built and gated, not burned) | ✅ Shipped and **enforced** (⚠️ NXE is asserted by the APs but only *inherited* on the BSP, never verified — § 1.1) | `agnos/kernel/core/proc.cyr:2386-2445` · `core/elf.cyr:281-298` |
| Port I/O denied to ring 3 — IOPL 0 and **no I/O permission bitmap** (I/O-map base 104 > TSS limit 103) | ✅ Shipped at **1.56.52**; ⛔ through 1.56.51 the base was written to the wrong TSS offset and ports `0x000–0x33F` were open to ring 3. Boot gate `tss: I/O map base at 0x66 OK` printed on iron at the 1.56.53 burn | `agnos/kernel/arch/x86_64/gdt.cyr:163-187` · `arch/x86_64/ring3.cyr:209-214` |
| Raw block-device writes refuse until deliberately armed | ✅ Shipped, ⚠️ **anti-accident, not authorization** — the arm token is a constant published in the source and any ring-3 process may present it; the arm clears on `blk_close` (1.56.51) and when the arming process dies (1.57.1 / 1.57.7) | `agnos/kernel/core/syscall.cyr:8616-8641,8673-8698,8820-8832` |
| Install-time package verification — SHA-256 root hash, Ed25519 signature over it, per-file content re-verify, then a trust gate | ✅ Shipped | `ark/src/ark_package.cyr:252-272,349-375,607-617` |
| — ⚠️ caveat: `require_signed` **defaults to 0**, so an unsigned `.ark` installs from a local path or any non-marketplace source. No config key parses it; the policy is code-settable only. The **one** path that forces it is `ark --marketplace <URL>`, which sets it to 1 unconditionally — "always require a trusted signer (fail closed), regardless of the global default"; a marketplace step run from an `--apply` plan does not (`ark/src/exec.cyr:176-200`) | ⚠️ partial | `ark/src/types.cyr:342` (default 0) · `ark/src/marketplace.cyr:56-58` (forced 1) |
| Foreign-source hash pinning — fetch → verify → *only then* extract; fail-closed on a hash that is not exactly 64 hex chars | ✅ Shipped | `takumi/src/cli.cyr:273-290` · `takumi/src/source.cyr:201` |
| — coverage: most zugot recipes carry a populated `sha256`; the ones with no `sha256` key are all `local = true` (no upstream source), but some remote recipes carry `sha256 = ""` under the validator's `# TODO` carve-out — un-pinned and unbuildable as written. The parser refuses a remote source whose hash key is absent | ⚠️ partial | `takumi/src/parse.cyr:203-218` · [`vulnerability-management.md`](vulnerability-management.md) |
| Hash-chained audit records, with opt-in per-entry Ed25519 signing | ✅ Shipped as a library (§ 4.1 caveats) — `chain_append` hashes and links but does not sign; signing is a separate opt-in | `libro/src/chain.cyr:157-166,349-366` |
| Build sandbox — netns hermeticity, Landlock write-confinement, group-kill timeout | 🐧 **Linux-target only**, and ⚠️ **self-declared NOT a containment boundary** against malicious recipes (recipes are trusted). Layers are best-effort: an unavailable layer runs the step *degraded with a warning* unless `--require-sandbox`. Known gap SEC-11 — a step that double-forks / `setsid()`s escapes the group-kill and outlives the build | `takumi/src/sandbox.cyr:1-33` |
| **KASLR** | ❌ **Absent in the shipped build** | see below |
| IOMMU / DMA restriction | 🐧 / ⚠️ **not active on the primary iron target** | see below |
| Confinement, MAC, per-process capabilities | 📋 **Aspirational** | `agnos/kernel/core/syscall.cyr:8616-8629` — the arm is "the exact seam where an aegis installer-capability check lands", and since 1.56.56 its named successor is a **kavach capability gate in userland**, not a kernel credential (through 1.56.55: "when agnos grows per-proc caps") |
| Per-child resource caps (memory pages, CPU time) | ⚠️ Since 1.57.7, built and gated, **not burned** — opt-in by the parent (`spawn_limits#107`); shm, GPU slots, channel rings, pipe buffers, page tables and kernel stacks are not counted | `agnos/kernel/core/syscall.cyr:10499-10514` · `core/proc.cyr:3137-3143` |
| Signature verification at **exec** | ❌ Absent | `agnos/kernel/core/elf.cyr:145-174` validates ELF magic, 64-bit class and bounds. No hash, no signature |
| Secure Boot / measured boot / TPM | 📋 **Aspirational, post-v1.0** | `gnoboot/docs/development/roadmap.md:366-380,406-410`; gnoboot's `[deps]` is `stdlib = ["fnptr"]` — it links no crypto, and since 0.7.0 checks only the loaded ELF's structure |
| Firewall / packet filter | ❌ Absent | No netfilter, nftables or filter hook in the AGNOS kernel |

**KASLR — why it is ❌ and not ✅.** The mechanism exists, but the image-base slide requires an opt-in `CYRIUS_PIE=1` build. That flag appears **nowhere** except the smoke script's own usage text (`agnos/scripts/smoke/kaslr-smoke.sh:7,26,29`) — not in agnos CI, not in `build.sh`, not in this repo's boot/ISO pipeline (re-checked at 1.57.10). `readelf -h agnos/build/agnos` reports `Type: EXEC`, and gnoboot slides only `ET_DYN`. The allocator half is independently dead: `pmm_next_free` is RDRAND-seeded (`agnos/kernel/core/pmm.cyr:535-537`) but no allocator reads it. **No address randomization is active in a shipped AGNOS today.**

**IOMMU — why it is qualified.** VT-d is Intel-only, and it **never enabled translation on any boot through 1.57.9**; since 1.57.10 `iommu_init()` runs every boot and turns translation on when a drivable DMAR unit is present (`agnos/kernel/core/main.cyr:717-736`) — gated in QEMU only, **not burned**. On AMD, AGNOS **actively disables AMD-Vi on every boot** — `amd_iommu_disable()` writes the AMD-Vi Control Register to 0, "passthrough for everyone" (`core/main.cyr:628-636`). The primary iron target is AMD. **On the hardware AGNOS actually runs on, DMA is unrestricted.**

---

## Host-bootstrap kernel hardening (🐧 Linux only)

These `CONFIG_*` symbols are genuinely present in `kernel/6.6-lts/configs/agnos_defconfig` and were re-verified line by line. They harden the **Linux host used to build AGNOS**. ⚠️ **AGNOS has zero LSMs and no LSM framework** — do not read this table as an AGNOS posture.

| Symbol | Line | Note |
|---|---|---|
| `CONFIG_SECURITY_SELINUX=y` | :21 | |
| `CONFIG_SECURITY_LANDLOCK=y` | :22 | kavach's Landlock backend targets this host, not AGNOS |
| `CONFIG_SECURITY_YAMA=y` | :25 | |
| `CONFIG_SECURITY_SAFESETID=y` | :26 | |
| `CONFIG_SECURITY_LOCKDOWN_LSM=y` | :27 | |
| `CONFIG_INTEGRITY=y` | :28 | |
| `CONFIG_SECCOMP=y` / `CONFIG_SECCOMP_FILTER=y` | :66-67 | The real upstream symbols |
| `CONFIG_SECURITY_SECCOMP=y` | :23 | ⚠️ **Not an upstream Kconfig symbol.** A defconfig line for a nonexistent symbol is silently dropped by `make olddefconfig`; the working seccomp pins are :66-67 |

⚠️ **Known gap in this set.** [`SECURITY.md`](../../SECURITY.md) says the host defconfigs are meant to pin `# CONFIG_CRYPTO_USER_API{,_HASH,_SKCIPHER,_AEAD,_RNG} is not set` to strip the CVE-2026-31431 surface from bootstrap hosts (until 2026-09-27 it said they already did), and `docs/development/roadmap.md:73` lists that pin as a **Do** item. It has not been done (re-checked 2026-09-27): a search across all nine host config files (`kernel/{6.6-lts,6.x-stable,7.0-devel}/configs/*` and `kernel/configs/edge-*.config`) returns **zero** `CRYPTO_USER_API` lines. The AGNOS kernel's structural immunity is unaffected — it has no AF_ALG to begin with — but the **host** kernels do not currently carry the pin.

## Verification

⚠️ **There is no `agnos-cis-audit` binary.** The predecessor documented three `sudo agnos-cis-audit …` invocations; nothing by that name exists anywhere in the repo, and AGNOS has no `sudo` (shakti is the escalation tool, is pre-1.0, and is Linux-only). The nearest historical artifact is `scripts/archive-pre-cyrius/cis-validate.sh` — archived Rust-era bash whose helpers shell out to Linux `sysctl`.

📋 **No automated CIS auditing exists for AGNOS, and no CIS assessment has ever been run against it.** What an operator can verify today is the AGNOS-native table above, control by control, by reading the cited source. Supply-chain integrity has real tooling: `cyrius deps --verify`, `cyrius vet`, `cyrius deny`, and `scripts/check-dep-tags.py` — documented in [`vulnerability-management.md`](vulnerability-management.md).

## Summary

Of roughly 55 controls in the predecessor's mapping, **one family (3.4, uncommon protocols) maps in a way AGNOS satisfies — and satisfies structurally rather than by configuration.** Three families map onto a *different* AGNOS mechanism with materially different properties (time sync → kernel SNTP; auditd → libro; syslog → sakshi + console). The remainder presuppose uid/gid, PAM, systemd, sysctl, mount options or a distribution package set, none of which AGNOS has or plans to acquire.

The correct posture statement is the one in [`SECURITY.md`](../../SECURITY.md): AGNOS is a **single-owner, single-trust-domain** OS whose security value is a radically small, sovereign attack surface that structurally lacks whole classes of Linux vulnerability — **not** the enforcement of boundaries between users or processes, which the kernel does not yet implement. A CIS mapping measures the second thing. It is the wrong instrument for this system, and this document exists to say where and why, not to produce a score.

## References

- CIS Distribution Independent Linux Benchmark v3.0.0 — the source of the control families mapped above
- DISA STIG — a **separate** standard (the predecessor listed it as a CIS document; it is not)
- [`SECURITY.md`](../../SECURITY.md) — authoritative security posture, listening set, and compliance table
- [`docs/development/roadmap.md`](../development/roadmap.md) — permanent boundaries, incl. the native-confinement commitment
- [`docs/security/vulnerability-management.md`](vulnerability-management.md) — the real supply chain and its verification points
