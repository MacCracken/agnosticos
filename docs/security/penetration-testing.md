# AGNOS Penetration Testing Framework

**Last Updated**: 2026-09-27

> **Re-verified 2026-09-27** against agnos 1.57.10 (built and gated in QEMU, **not burned** — the last iron
> burn is 1.57.1) / cyrius 6.6.6 / gnoboot 0.7.2. Several targets below changed underneath this plan between
> 1.56.40 and 1.57.10 — sockets gained owners, ring-3 port I/O and user-pointer validation were fixed, `fork`
> shipped — and each says which kernel it applies to. Record the kernel version you test, always.
>
> **Rewritten 2026-08-05** against agnos 1.56.40 / cyrius 6.5.7 / gnoboot 0.6.1. The prior version of
> this document (2026-03-07) scoped a Rust-era system: Landlock/seccomp/namespace sandbox escape, `sudo -l`,
> SUID hunting, `/proc/self/ns` inspection, mTLS, a Wayland compositor, and loadable kernel modules.
> **None of those exist on AGNOS.** A pen-test plan aimed at defenses that were never built is worse than
> no plan — an empty `find / -perm -4000` reads as *hardened* when the truth is *there is no uid model at all*.
> Every target below was re-derived from source. Every control carries a status marker and a path+line.

## Status markers

| Marker | Meaning |
|---|---|
| ✅ | **Shipped and verified** — I opened the source and saw it |
| 🐧 | **Linux-target only** — real on a Linux host, compiles out on AGNOS |
| 📋 | **Intended / roadmap** — does not exist today |
| ⛔ | **Absent** — no such mechanism, by design or by omission |
| ⚠️ | **Present but not enforced** — the code exists and does not gate anything |

---

## The posture a tester must start from

**AGNOS today is a single-owner, single-trust-domain OS.** Every process runs as uid 0, every process is
trusted, and the machine — not the process — is the security boundary.

- ⛔ **No uid model.** `agnos/kernel/core/syscall.cyr:9667` — `if (num == 15) { return 0; }  # getuid (root)`.
  There is no `setuid`, no `geteuid`, no `st_uid`, no `/etc/passwd` — and since agnos 1.56.56 that is recorded
  as the design, not a gap (`planning/identity-and-authorization-model.md`).
- ⛔ **No per-process capabilities.** `agnos/kernel/core/syscall.cyr:8616-8629` states it in-source: *"agnos has
  no per-proc capability/uid yet … the arm call is the exact seam where an aegis installer-capability check
  lands"* — and names the successor as *"a KAVACH capability check in userland"*, not a kernel credential (the
  1.56.56 correction; through 1.56.55 it read "when agnos grows per-proc caps").
- ⛔ **No confinement.** kavach's Landlock / seccomp / namespace / cgroup arms are `#ifdef CYRIUS_TARGET_AGNOS`-gated
  to fail closed on AGNOS — `kavach/src/confine.cyr:118` (`return 0 - 1`), `:348` (`sys_exit(SPAWN_EXIT_NNP)`),
  `:435-437`, `:477-479`, `:549-551` (kavach tree on 2026-09-27). Its own comment names the reason: *"a sandbox
  that is silently not a sandbox, which is the worst failure mode this file has."* 🐧 on Linux those backends
  are real and work.
- ⛔ **No signature verification at exec.** `agnos/kernel/core/elf.cyr:145-174` and `:453-...` (`elf_load_from_file`)
  validate ELF magic, 64-bit class, an entry floor of `0x400000`, and program-header bounds. No hash. No
  signature. No sigil reference anywhere in the loader.

### Test plans that DO NOT APPLY — do not budget time for these

| Classic plan | Why it is void here |
|---|---|
| Privilege escalation (`sudo -l`, SUID hunt, `getcap -r /`) | ⛔ Everything is already uid 0. There is nothing to escalate *to*. `find / -perm -4000` returning empty is not a finding. |
| Sandbox / container escape (`/proc/self/ns`, cgroup, Seccomp status) | ⛔ No `/proc`, no namespaces, no cgroups, no seccomp. Empty output means *no mechanism*, not *isolated*. |
| `LD_PRELOAD` / dynamic-linker abuse | ⛔ Static binaries only; no dynamic linker. |
| Kernel-module loading / rootkit insertion | ⛔ The kernel is a single Cyrius binary. No loadable modules, no eBPF. |
| `ptrace` injection | ⛔ No `ptrace` syscall exists. |
| Copy-on-write abuse on process creation | ⛔ There is no copy-on-write. ⚠️ `fork`#96 **does exist** since agnos 1.56.54 (corrected 1.56.55) — it was reserved and unbuilt when this row was first written — but it is a full address-space copy, so there is no CoW fault path to race. The fork itself is ordinary surface now: test it as such (`agnos/scripts/smoke/fork-smoke.sh`). |
| mTLS downgrade / client-cert confusion | ⛔ No mTLS anywhere in the ecosystem. Inter-service traffic is plain HTTP. |
| PQ key-exchange downgrade / hybrid handshake attack | ⛔ There is no PQ key exchange. `ml_kem` / `\bkyber\b` / `sphincs` return **zero hits** across `sigil/src` (a bare `kyber` matches only "kybernet"). The only real PQ surface is **ML-DSA-65 signing** (`sigil/src/lib.cyr:146-168` — default-on since 3.7.6, the `-D SIGIL_PQC` flag is a no-op). |
| Wayland protocol fuzzing against the compositor | ⛔ aethersafha speaks the sovereign **setu** protocol (`aethersafha/cyrius.cyml:174` `[deps.setu]`). Every `wayland` hit in its source is an env var handed to a *guest Linux* app (`src/apps.cyr:635`, `:639`, `:643` `MOZ_ENABLE_WAYLAND=1`; `:647`, `:727-728` `QT_QPA_PLATFORM` / `GDK_BACKEND`), a plugin-capability bit (`src/plugin_host.cyr:69`) or a comment — lines as of aethersafha 0.16.27. None is compositor protocol code. |

**The interesting targets are elsewhere:** memory safety in an unchecked language, the syscall boundary,
the unauthenticated service surface, the agent/MCP layer, and the boot chain.

---

## Scope

### In Scope — the surface AGNOS actually presents

**1. The syscall boundary** — as of agnos 1.57.10 the numbers in use are `#0`–`#108` with `#105` withdrawn; the
authoritative table is `agnos/docs/development/agnos-userland-abi.md`, not a count copied from here. Most dispatch
from `ksyscall` in `agnos/kernel/core/syscall.cyr` (`#97 chan_op` at `:10114`); `#44 sched_yield` and
`#96 fork` dispatch from `arch/x86_64/syscall_hw.cyr:101,106` instead. All reachable from ring 3 — **there is no
privilege partition of the number space.**

**2. The network band** — fixed-shape, slot-indexed, **not a BSD socket family**:
`#45 getrandom · #46 time_unix · #47 sock_connect · #48 sock_send · #49 sock_recv · #50 sock_close ·
#51 udp_bind · #52 udp_send · #53 udp_recv · #54 udp_unbind · #55 icmp_echo · #56 sock_listen · #57 sock_accept`,
plus `#100 icmp_echo_ex` and `#106 sock_peer` (`agnos/kernel/core/syscall.cyr:10489-10498, 11417-11744`).
`conn_id` is an index into an 8-slot global table — and since **1.57.7** (built and gated, not burned) each id
is owned by the process incarnation that opened it. ⛔ No `socket()` over arbitrary domains, no address-family
argument, no `splice`, no `AF_ALG`, no `AF_UNIX`. **Never write "AGNOS has no sockets" — it has this band, and it
works.**

**3. Listening services** — enumerated below with their real bind addresses and real authentication state.

**4. The MCP / agent layer** — prompt injection, tool abuse, model supply chain.

**5. Memory safety in the kernel and userland** — Cyrius is everything-is-`i64` with raw
`load8`/`load64`/`store8`/`store64`. ⛔ No bounds checking, no pointer types, no ownership, **no automatic stack
protector**. Every bound is a hand-written `if`.

**6. Install-time package verification (ark)** and the total ⛔ absence of exec-time and boot-time verification.

**7. The boot chain** — gnoboot 0.7.2 and what it does *not* check.

**8. Physical / DMA** — because ⛔ the IOMMU is off on the primary iron target (see below).

### Out of Scope
- Physical security beyond the DMA surface named above
- Social engineering
- Guest Linux applications running under the swallow path (they are the *host's* problem, not AGNOS's)
- Third-party cloud services (unless AGNOS-managed)
- **Any Linux-host build of an AGNOS component.** 🐧 kavach, shakti and phylax have substantial Linux-only
  confinement/auth surfaces. Testing those tests Linux, not AGNOS. Say which target you tested, always.

---

## Target 1 — Listening services

Enumerated from `sock_listen` / `sandhi_server_run*` / `INADDR_*` across `/home/macro/Repos/*/src/`
(re-swept 2026-09-27). ⛔ **"Nothing else listens" is false.** Several services bind `0.0.0.0` by default.
(The `INADDR_ANY` sweep also hits `sandhi` — the HTTP server *library*, which exposes the bind address
as an API, not a service — and `mirshi`, which binds `INADDR_ANY` only behind an explicit
`--net-listen-any` flag (`mirshi/src/dispatch.cyr:85`, `:555`) and is loopback by default; `agnostic` does
the same only when `AGNOSTIC_ADDR=any`.)

⚠️ **On an AGNOS target the Bind column is what each service *asks for*, not what it gets.** Through cyrius
6.6.6 the stdlib's agnos `sock_bind` drops the address (`cyrius/lib/net.cyr:295-302`) and `sock_listen`#56
is issued with the port alone (`lib/syscalls_x86_64_agnos.cyr:1633-1641`), so every "loopback" row below is a
listener on the NIC's address when built for AGNOS. Kernel 1.57.7 added a loopback-only listen class (built
and gated, not burned). cyrius 6.6.7 (2026-09-27) fixed the stdlib half: the bind address now selects the listen class — 127/8 → the loopback-only class, `0.0.0.0` or the host's own address → any, any other address refused (`cyrius/lib/net.cyr:300-313` in 6.6.8; `cyrius/CHANGELOG.md` [6.6.7], *Security*). A service carries that only once rebuilt on 6.6.7 or later — daimon,
hoosh and bote still pin 6.6.6 as of 2026-09-28. Test loopback claims from a second host.

| Service | Port | Bind | Auth | Verified at |
|---|---|---|---|---|
| **daimon** `serve` | 8090 | ✅ `127.0.0.1` since daimon **2.3.0**; ⛔ `INADDR_ANY` through 2.2.x | ⛔ **NONE** | Bind from `server_bind_addr()` (`daimon/src/server.cyr:260-264`) — `listen_addr`, default `"127.0.0.1"` (`src/config.cyr:10`), `serve --listen` overrides; port at `src/config.cyr:11`. Through 2.2.x both serve modes passed `INADDR_ANY()` and that default was dead code. On AGNOS it lands on the NIC (note above) and daimon audits `http.listen.not_loopback` (`server.cyr:1187-1194`). No inbound auth: `src/api_agent.cyr:11-12` says so, pending daimon's roadmap 2.5.x. |
| **hoosh** `serve` | 8088 | ✅ `127.0.0.1` (`0x0100007F`, `src/main.cyr:88`; `INADDR_ANY` before hoosh 2.5.1) | ⚠️ **fails open by default** | `hoosh/src/lib/auth.cyr:33-36` — `if (vec_len(tokens) == 0) { return 1; }`. With no tokens configured every request is allowed. The warning at `main.cyr:368-374` fires only off-loopback *and* with no tokens; the default bind does not warn. |
| **bote http** | 8390 | ✅ `INADDR_LOOPBACK()` | ⚠️ opt-in only | `bote/src/main.cyr:143-148`. Bearer wired **only if** `$BOTE_BEARER_TOKENS` is set (`src/main_common.cyr:97-105`); unset → `bearer_fp = 0` → no validator installed → everything passes. |
| **bote bridge** | 8391 | ✅ `INADDR_LOOPBACK()` | ⚠️ same, **plus CORS `Origin: *`** | `bote/src/main.cyr:155-161` pushes `"*"` into the allowed-origins vec. |
| **bote-streamable** / **bote-ws** | 8392 / 8393 | ✅ loopback | ⚠️ same opt-in bearer | separate binaries. **A bare `bote` with no argument runs stdio, not a listener** (`main.cyr:168`) — a port opens only on an explicit subcommand. |
| **agora** (telnet BBS) | 2323 | ⛔ `INADDR_ANY` | ✅ sigil Ed25519 challenge/response, **anon-read / auth-post** | `agora/src/main.cyr:36` (`DEFAULT_PORT`), `:3186` (bind), `:700-748` (login verb). The only service in the set whose auth is **on by default** and challenge/response rather than a shared bearer secret. |
| **vidya** | 8390 | ⛔ `INADDR_ANY` | ⛔ none found | `vidya/src/main.cyr:323`, `:475` (bind). ⚠️ **Note the collision** — same port number as bote-http, but public rather than loopback. |
| **hadara** | 8391 | ⛔ `INADDR_ANY` | ⛔ none found | `hadara/src/main.cyr:407`, `:924`. Same collision with bote-bridge. |
| **mneme** MCP | 8100 | ⛔ `INADDR_ANY` | ⛔ none found | `mneme/src/main.cyr:42`, `src/mcp_server.cyr:274` |
| **agnosai** | 8080 | ⛔ `INADDR_ANY` | ⚠️ **real, and OFF by default** | `agnosai/src/main.cyr:127` (port), `:428`/`:430` (bind). Unlike the others, agnosai *ships* auth: bearer-token **and** JWT/RS256, with a constant-time secret compare (`src/server/auth.cyr`, ADR 009), wired into the request path by `agnosai_route_needs_auth` (`src/server/router.cyr:464-470`, gate at `:553-558`). But `agnosai_auth_config_new` stores `enabled = 0` (`auth.cyr:61-67`) and the file says so plainly: *"the default is fail-**open**, so a caller that forgets to configure auth serves every route unauthenticated"* (`auth.cyr:56-60`). Enabled only by `AGNOSAI_AUTH_ENABLED` / `AGNOSAI_AUTH_SECRET` / a JWT config (`main.cyr:155-224`). **Do not report "agnosai has no auth" — report that its auth is off unless three env vars say otherwise.** |
| **cyrius-yeomans-descent** (MUD) | 4000 | ⛔ `INADDR_ANY` | ⚠️ per-character **passphrase** login; no challenge/response | `cyrius-yeomans-descent/src/server.cyr:15` (`DEFAULT_PORT = 4000`), `:211` (bind). A telnet MUD server parsing untrusted input in an unchecked language — the same exposure class as agora, **without** agora's sigil challenge/response: the pre-login stream is unauthenticated, and login is a passphrase sent in the clear over telnet (`player_auth_load`, `src/persist.cyr:2395-2404`; `MAX_LOGIN_FAILS = 5`, `src/session.cyr:228`). ⚠️ An earlier revision of this row called `player_auth_*` savefile field clamping — `player_auth_load` is the passphrase authenticator. A high-value MEM-05 target. |
| **kavach** credential proxy | caller-chosen | ✅ `INADDR_LOOPBACK()` (`src/credential_http.cyr:97`) | ⛔ **NONE** — allowlist by secret *name* only | `GET /v1/secret/<name>` returns the raw secret. Header (`:10-11`) argues *"Localhost-only … a peer outside the same loopback namespace can't reach the listener."* ⚠️ **On AGNOS there are no namespaces and no uid — every process on the box is a loopback peer**, and a loopback bind is not honoured there anyway (note above). Loopback is not an isolation boundary here. |
| **phylax** daemon | — | AF_UNIX | ⛔ none | `phylax/src/cli.cyr:967-1040`. ⛔ **Cannot run on AGNOS** — no `AF_UNIX`. |

### Tests
- [ ] SVC-01 — Reach daimon 8090 from another host; confirm unauthenticated agent control (**expected to succeed on AGNOS**, where the loopback bind is not honoured; on a Linux host expected to succeed only against daimon ≤ 2.2.x or an explicit `serve --listen 0.0.0.0`)
- [ ] SVC-02 — Reach hoosh 8088 with no `Authorization` header; confirm `auth_check` fail-open (**expected to succeed**)
- [ ] SVC-03 — bote with `BOTE_BEARER_TOKENS` unset: invoke a tool with no credential
- [ ] SVC-04 — bote-bridge CORS `Origin: *` — browser-origin tool invocation from an attacker page
- [ ] SVC-05 — kavach credential proxy: fetch every allowlisted secret name from an unrelated local process
- [ ] SVC-06 — agora 2323: attempt post without completing the Ed25519 challenge; test challenge replay and the 30 s parked-login deadline (`agora/src/main.cyr:171-178`)
- [ ] SVC-07 — Port collision: 8390/8391 are claimed by both a loopback bote and a public vidya/hadara. Determine which binds first and whether a low-privilege service can pre-empt the loopback one
- [ ] SVC-08 — Timing/side-channel on bote's bearer compare (it is constant-time at `bote/src/auth.cyr:97-117` — **verify the claim, do not assume it**; note the enclosing `if (strlen(cand) == tlen)` still branches on length)
- [ ] SVC-09 — agnosai 8080 with `AGNOSAI_AUTH_ENABLED` unset: confirm every route serves unauthenticated on a public bind (**expected to succeed**). Then set the secret and re-test the *enabled* path — a shipped-but-off control is a config finding, not an absent-control finding, and must be reported as such
- [ ] SVC-10 — cyrius-yeomans-descent on 0.0.0.0:4000: unauthenticated pre-login parsing (telnet negotiation, name prompt) and the cleartext passphrase login; fuzz the telnet line handler (see MEM-05)

---

## Target 2 — The syscall boundary

This is the highest-value target on the system. It is the *only* ring-3 → ring-0 transition and it is
hand-written.

**What validates a userland pointer** — the trust boundary is these functions (agnos 1.57.10):

```
agnos/kernel/core/syscall.cyr:354   user_range_mapped(cr3, ptr, len) -> PML4/PDPT/PD walk: present + U/S=1 on every 2 MB page
agnos/kernel/core/syscall.cyr:509   is_user_ptr(ptr)        -> is_user_range(ptr, 1)
agnos/kernel/core/syscall.cyr:513   is_user_range(ptr, len) -> len >= 0, wrap check, floor 0x200000, window
                                                               (low <= 0x40000000, or the high arena), then the walk
agnos/kernel/core/syscall.cyr:560   is_user_array(p, n, sz) -> n <= 0x40000000 / sz before multiplying, then is_user_range
agnos/kernel/core/syscall.cyr:594   sc_path_ok(ptr, len)    -> len >= 1 && is_user_range(...)
agnos/kernel/core/syscall.cyr:609   sc_env_blob_ok(buf,len) -> kernel-copy env validator, 16-entry cap
```

⛔ **Through 1.56.51 the first three were pure VA-window checks**, and under every per-process CR3 the window
also holds the kernel's supervisor identity pages — so `write(1, 0x300000, 4096)` disclosed the kernel image and
a `read()` onto a syscall kernel stack gave ring-0 execution (measured and fixed at 1.56.52; `syscall.cyr:318-353`
records it). `is_user_array` (1.56.51) closed the `count × stride` wrap class in `readdir`, `proclist` and the
raw-block arms. A finding reproduced against a pre-1.56.52 kernel is history, not news — say which kernel.

✅ `is_user_range` is the workhorse — called from every buffer-taking arm in `syscall.cyr`, plus `gpu.cyr`,
`vfs.cyr` and `ext2.cyr`. ⚠️ `is_user_ptr` has exactly **one** caller left in the whole kernel, and it is not a
gate: `core/main.cyr:4432`, a `shsys` selftest asserting that the ceiling rejects `0x40000000`. No syscall arm
calls it.

⛔ **There is no copy-in.** Handlers `load8`/`store64` **directly against the user VA under the caller's CR3**,
inside a single SMAP window the entry stub opens once: `stac` immediately before `call syscall_handler`,
`clac` immediately after (`agnos/kernel/arch/x86_64/syscall_hw.cyr:612-621`). **SMAP is disabled for the entire
duration of every syscall**, not per-copy as Linux does it. Any handler that re-reads a user pointer after a
bounds check has a live TOCTOU window with no structural mitigation.

**Entry-path hardening that IS real:**
- ✅ **SFMASK = 0x40700** clears `IF|TF|DF|AC` on entry (`syscall_hw.cyr:364-376`). The in-source comment names
  each attack it closes: TF single-steps the kernel into the bare-`iretq` #DB gate, DF reverses rep-string ops,
  AC pre-disables SMAP ahead of the stub's STAC.
- ✅ **SMEP + SMAP**, CPUID-gated, set in the Path C boot shim (`arch/x86_64/boot_shim.cyr:308-348`).
  ⚠️ Silently skipped on a CPU that does not advertise the bits — no warning, no assertion.
- ✅ **IOPL 0 with no I/O permission bitmap — since 1.56.52.** The I/O-map base is stored as **104** at TSS
  offset **102** (`arch/x86_64/gdt.cyr:183`) while the TSS descriptor limit is **103** (`gdt.cyr:187`) — the
  base lands *past* the limit, which is the x86 encoding for "no bitmap". IOPL itself is 0: scheduled ring-3
  procs start with RFLAGS `0x200` (`proc.cyr:1096`, `:1160`, `:1195`) and the pre-scheduler foreground entry
  with `0x002` (`arch/x86_64/ring3.cyr:214`) — bits 12-13 clear. Any `in`/`out` from ring 3 takes #GP. ⛔
  **Through 1.56.51 this was a live hole**: the 104 was written to offset 100 (a reserved field), the real base
  stayed 0, and the CPU read a bitmap out of the zero-filled TSS — ports `0x000–0x33F` (reset via `0x64`, CMOS,
  the 8259 masks, the PIT) were open to ring 3 while `0x3F8` still faulted, which is why it survived. A boot
  gate now prints `tss: I/O map base at 0x66 OK - no ring-3 port access`; it did so on iron at the 1.56.53 burn.
- ⚠️ **Stack canaries are FOUR hand-placed guards**, not `-fstack-protector`. As of 1.57.10
  `var canary = stack_canary_secret` appears in exactly four functions: `elf.cyr:146`, `elf.cyr:454`,
  `syscall.cyr:8871` (`spawn_load_child`, 1.57.7) and `syscall.cyr:9417` (`ksyscall`); `net_handle_tcp`'s was
  removed at 1.56.52. The secret is a **global**, not `gs:0x28` — readable by any kernel-memory disclosure.

**Missing authorization inside the surface — verified, not inferred:**

| Band | Gate | Status |
|---|---|---|
| `kill #16` | `proc_may_signal(caller, target)` — init, self, or **direct child only**, epoch-validated since 1.57.7 | ✅ `proc.cyr:1957-1976`. ⚠️ Since 1.57.7 (built and gated, not burned) it *acts* — 9 ends, 19 stops, 18 continues, `0x100` reaches descendants; through 1.57.6 it only set a pending bit |
| `shm_free #74` | owner-gated, non-owner gets `-1` | ✅ `syscall.cyr:10298-10310` |
| `shm_write #72` / `shm_read #73` | ⛔ **cross-owner permitted**, warn-counted only | `syscall.cyr:10284-10297` — the in-source rationale is the compositor reading client buffers |
| `sock_send #48` / `sock_recv #49` / `sock_close #50` / `sock_accept #57` / `sock_peer #106` | ✅ **owner-checked since 1.57.7** (built and gated, not burned) — the id belongs to the pid + epoch that opened it; anyone else gets −1. ⛔ Through 1.57.6: bounds-checked `conn_id` only — any proc could send on, drain, or tear down any of the 8 global conns | `syscall.cyr:10489-10498, 11489-11552, 11712-11744` |
| `udp_recv #53` / `udp_unbind #54` / `udp_send #52` | ✅ owner-checked since 1.57.7, built and gated, not burned (`#52` refuses a source port another owner bound). ⛔ Through 1.57.6: `listener_id` bounds only | `syscall.cyr:11569-11641` |
| `blk_write #78` | ⚠️ **interlock, not authorization** | `syscall.cyr:8590-8698` — any ring-3 proc can call `blk_open(_, 0x424C4B5F5257)` (a constant published in the source) and arm system-wide raw sector writes. Through 1.56.50 that arm lasted the whole boot; since 1.56.51 `blk_close` clears it (`:8820-8832`), and since 1.57.1 / 1.57.7 the arming process's death does (`:8632-8641`). **Anti-accident, not anti-adversary — the source says so.** |
| `blk_read #77` | ⛔ ungated by design ("non-destructive") | `syscall.cyr:8704-8758`. Non-destructive is not non-disclosing: this reads the whole disk. |
| `sys_mmap` | ⚠️ **no per-process page budget by default** — only a 64 GB single-call cap and total free 2 MB regions. Since 1.57.7 (built and gated, not burned) a parent can cap a child with `spawn_limits #107`; opt-in, and blind to shm, GPU slots, channel rings, pipe buffers, page tables and kernel stacks | `proc.cyr:3126-3143` |
| `power_sys #13` | magic-pair gate only, from any pid | `core/power.cyr:410-430` — *"`getuid` is hardcoded 0 and there is no uid model, so a uid check would be a gate that is always open."* Since 1.56.56 the same comment calls that permanent. |

### Tests
- [ ] SYS-01 — Fuzz every syscall arm with pointers at the exact `is_user_range` boundaries: `0x1FFFFF`, `0x200000`, `0x3FFFFFFF`, `0x40000000`, and wrap-inducing `(ptr, len)` pairs — and, since the 1.56.52 page walk, with low-window VAs that are *mapped but supervisor* (the kernel image at `0x300000`, the kernel-stack pool at `0xE00000`): each must be refused. The `userwin` smoke gates both directions
- [ ] SYS-02 — **The high arena.** `sys_mmap` spills into a HIGH arena at `[0x2000000000, 0x8000000000)` = [128 GB, 512 GB) (`USER_HIMMAP_BASE` / `HIMMAP_CEILING` at `proc.cyr:3088-3089`) since 1.50.2. Through 1.56.51 every validator rejected a high pointer (failing closed). Since 1.56.52 `is_user_range` admits `[himmap_floor(), HIMMAP_CEILING)` — the floor is derived from what the direct map installed — and relies on the page walk's U/S check; `is_user_array` still bounds its count against the low window. Confirm a high pointer is accepted only for pages the caller owns, and hunt for any arm that names direct-map memory
- [ ] SYS-03 — TOCTOU across the single open SMAP window: re-map or unmap a page between a handler's bounds check and its dereference
- [ ] SYS-04 — Cross-process shm read/write via `#72`/`#73`; confirm another process's live buffer is readable
- [ ] SYS-05 — Hijack another process's TCP connection via `sock_send`/`sock_recv`/`sock_close` on a slot you never opened. **Expected to succeed through 1.57.6 and to fail since 1.57.7** (owner check first, −1) — on 1.57.7+ a success is a regression; also try a forked or spawned child and a recycled pid, which the owner epoch must refuse
- [ ] SYS-06 — Arm `blk_rw_armed` from an unprivileged-intent process and write to sector 0
- [ ] SYS-07 — Memory-exhaustion DoS: `mmap` all machine RAM from one process (uncapped by default); then arm `spawn_limits #107` in a parent and confirm the child's `mmap` returns 0 at the cap and that shm, GPU slots, channel rings and pipe buffers are not counted
- [ ] SYS-08 — Proc-table slot leak toward the 16-proc cap (`proc_reap` reclaims only the top slot)
- [ ] SYS-09 — **Uninstalled exception vectors.** Of the exception vectors, only `{0,6,7,8,10,11,12,13,14,19}` are installed (`arch/x86_64/idt.cyr:152-165`, plus the lazy-FPU `#NM` at `core/main.cyr:280`); ring-3 faults are killed only for `{0 #DE, 6 #UD, 13 #GP, 14 #PF}` (`idt.cyr:565`). Everything else sits on a bare `iretq` that amplifies an error-code-pushing fault into #DF → halt. #DE proved this class costs a machine (fixed 1.56.18), and **so did ring-3 #GP until 1.57.7**: the stub's diagnostics dereferenced the saved user RSP and took a SMAP #PF at CPL0 (CPL guard at `idt.cyr:395-407`, built and gated, not burned). Enumerate which uninstalled vectors ring 3 can actually reach
- [ ] SYS-10 — **Newest code, least soak — prioritize it.** When this plan was written that was `chan_op #97` in the then-open 1.56.40 cycle (released since, and on iron from 2026-08-07, in the 1.56.41 kernel). As of 2026-09-27 it is **1.57.2 → 1.57.10, built and gated in QEMU but never burned**: per-process kernel stacks and their guard pages, the blocking model (every wait blocks only its caller), `kill` default actions and `KILL_TREE`, socket ownership and `sock_peer #106`, `spawn_limits #107`, `sched_yield_to #108`, and VT-d translation
- [ ] SYS-11 — Kernel-stack overflow. Since **1.57.6** (built and gated, not burned) each per-process kernel stack is a 64 KB region-7 slot whose lowest 4 KB is a not-present guard page (`proc.cyr:662-666`, `vmm.cyr:387-426`), and the shared per-CPU syscall stacks are gone — an overflow should fault into `#DF` on its IST1 stack. Confirm that on hardware; `kstack-smoke` only proves it in QEMU. ⛔ Through 1.57.5 there were **no kernel-stack guard pages**: RSP0 stacks were packed contiguously with the per-CPU syscall kstacks above them and an overflow walked silently into a neighbour with no #PF and no canary
- [ ] SYS-12 — Heap. 8 slab classes 32–4096 B. ⛔ No allocation header, no redzones, no guard pages, no double-free detection (`core/heap.cyr:1-164`). ⚠️ `kfree_sized`'s `size` must map to the same class as the `kmalloc` — an unenforced, load-bearing contract (`heap.cyr:135-144`); a smaller class under-scrubs and leaks the tail into a later allocation

---

## Target 3 — Memory safety in an unchecked language

The kernel is manual pointer arithmetic throughout: `&proc_table + pid * 176 + 168` (`proc.cyr:1279`),
`vfs_fd_table() + arg1 * 32` (`syscall.cyr:9773`). ⛔ No bounds checks, no types on pointers, no ownership.
The 1.41.5 audit found **ten HIGH findings of exactly this class**, including arbitrary kernel read *and* write
from ring 3 via `epoll_ctl`/`epoll_wait` type confusion — and the 1.56.51 / 1.56.52 audit sweeps found more of
it: the `count × stride` wrap class, the ELF PT_LOAD identity-shadowing class (ring 3 to arbitrary kernel write),
the VA-window-only pointer validator, and four ring-0 stack overflows in the in-kernel shell.

⚠️ **Those were found by audits, not prevented by a mechanism** — and the regression coverage did not run.
`SYSCALL_HARDEN_SELFTEST` shipped in 1.41.5, had no runner, and had **stopped compiling** — invisible for
~15 minor versions. A runner landed **2026-08-05** (1.56.40) and is now a `sweep.sh` row. Per the CHANGELOG's
own words: *"A selftest nothing runs is not coverage; it is a comment."*

### Tests
- [ ] MEM-01 — Type confusion on tagged kernel tables (the epoll class that produced the 1.41.5 HIGHs)
- [ ] MEM-02 — Integer overflow on every length/index arriving from ring 3
- [ ] MEM-03 — Use-after-free across the slab classes; `kfree_sized` class mismatch (SYS-12)
- [ ] MEM-04 — ⚠️ **SMP re-opens everything.** `agnos/docs/development/security-hardening.md:276-277`: *"SMP invalidates invariant (1). Every finding in this document judged 'not a race' must be re-judged when APs are scheduled."* APs are being scheduled. The two SMP-hole issues this item cited (filed 2026-08-02 and 2026-08-03) are **resolved and archived** — `agnos/docs/development/issues/archived/2026-08-02-large-image-ptload-pde-absent-smp.md` (the APs never set `EFER.NXE`) and `.../archived/2026-08-03-mmap-low-arena-global-cursor-smp-hole.md` (fixed at 1.56.36, a second defect at 1.56.53); both were already fixed when this item was written, but their status headers did not say so until 2026-08-07. The re-judgement itself is still owed, and 1.57.x added new concurrency to judge — per-process kernel stacks and the blocking model, built and gated, not burned. Re-run every race-adjacent test under `-smp 4`
- [ ] MEM-05 — Fuzz userland binaries too. Cyrius gives userland the same raw-pointer model, and the exposed instances are the ones parsing untrusted bytes off a public socket: **agora** (telnet, `INADDR_ANY`), **cyrius-yeomans-descent** (telnet MUD, `INADDR_ANY`, an unauthenticated pre-login stream and a cleartext passphrase login) and the sandhi-backed HTTP servers

---

## Target 4 — The agent / MCP layer

**⛔ Nothing is mitigated by default. Every defensive mechanism that exists is a library with no caller on this path.**

| Layer | Intended owner | Reality |
|---|---|---|
| Input scanning for encoded payloads | phylax | ⛔ `phylax/src/encoded_injection.cyr` does not exist |
| Gateway pre-flight + provenance tags | hoosh | ⛔ no `provenance` and no phylax reference in `hoosh/src/` |
| MCP capability-source policy | t-ron | ⛔ **t-ron is not wired into the MCP path.** `bote/cyrius.cyml` `[deps.*]` = libro, majra (sakshi and sigil arrive through the stdlib since bote 3.3.1) — **no t-ron**. `daimon/cyrius.cyml` = sakshi, ai-hwaccel, samay, bayan, sigil, libro, majra, bote, nein — **no t-ron**. Zero `tron_` symbols in `bote/src/` or `daimon/src/` (re-checked 2026-09-27). The dep runs the *other* way: `t-ron/cyrius.cyml` declares `[deps.bote-core]`. (thoth vendors t-ron for its own tool calls — `thoth/src/gate.cyr:388-438`, opt-in via `[tron].policy`) |
| Irreversible-action confirmation | kavach + agnoshi | ⛔ not present |
| Per-agent capability enforcement | daimon | ⛔ `grep -i capabilit daimon/src/` finds no enforcement — only edge-node capability reporting and a comment. `AgentHandle` (`daimon/src/agent.cyr:45-88`) has no capability field |
| `UntrustedInput<T>` / `Trusted<T>` types | agnostik | ⛔ not present |

📋 `docs/development/planning/agent-injection-defense.md` is a **design spine, not a control** — its own header
reads *"Status: Planning — Design Phase"*. Cite it as roadmap; never as a mitigation.

**What genuinely exists but is unwired here:** t-ron's six-check injection detector (`t-ron/src/safety.cyr:470-492`)
over a string with zero-width, bidi-control and BOM characters stripped (`:339-372`, a byte-level strip — no
NFKC folding), its token-bucket rate limiter, ACL policy and circuit breaker, with deny codes
`unauthorized / rate_limited / injection_detected / tool_disabled / anomaly_detected / parameter_too_large`
(`t-ron/src/gate.cyr:14-21`). ⚠️ **Nothing on the bote/daimon path calls any of it.**

**Model supply chain:** hoosh's default route is `http://localhost:11434` — Ollama (`hoosh/src/main.cyr:1048`).
✅ `tula` provides a sovereign weight format with a **sigil-signed Ed25519 header** — the one place a model
artifact can be integrity-anchored. ⚠️ Signing is optional and the readers check structure only; a caller
must call `tula_verify`. ⛔ Nothing in the agent path is *required* to use it, and `anukūlana` imports foreign
GPT-2 safetensors (and, since 1.1.0, GGUF) with its own parser and **no signature requirement**.

### Tests
- [ ] AGENT-01 — Prompt injection through hoosh into a daimon-driven agent (⛔ no L1/L2 scanning exists)
- [ ] AGENT-02 — Unauthenticated tool invocation over bote with `BOTE_BEARER_TOKENS` unset (⛔ no t-ron gate)
- [ ] AGENT-03 — **The full unmitigated chain**: reach daimon on 8090 with no credential (from the network on AGNOS, where its loopback bind is not honoured; on a Linux host, 127.0.0.1 since daimon 2.3.0) → drive an agent → invoke any bote MCP tool on loopback with no credential → no t-ron gate, no provenance tag, no per-agent capability, no sandbox (kavach no-ops on AGNOS), no kernel confinement (uid 0). Demonstrate end to end
- [ ] AGENT-04 — Poison a model checkpoint imported via anukūlana; confirm no signature is required
- [ ] AGENT-05 — Exfiltrate every kavach-proxied secret from an agent-spawned process
- [ ] AGENT-06 — Bypass t-ron's injection detector **as a library test** (zero-width stripping at `safety.cyr:339-372`, the 40 % special-char ratio, the 85 % base64 density heuristic, which also requires an `=`). This tests a *library*, not a deployed control — label the finding accordingly
- [ ] AGENT-07 — Audit-chain tampering. libro's chain makes tampering **detectable, not preventable**: `libro/src/file_store.cyr` is append-only JSONL guarded by `flock`, and on AGNOS every process is uid 0 with no MAC. Detection requires an out-of-band head-hash copy — libro ships `anchoring.cyr` / `timestamping.cyr` / `tpm_anchor.cyr` for that and 📋 **none is wired by default**

---

## Target 5 — The boot chain

⛔ **gnoboot 0.7.2 performs no cryptographic verification of anything.**

Since gnoboot 0.7.0's ELF-load hardening the checks are structural (`gnoboot/src/main.cyr:482-754`): the file
size (`:507-522`), the ELF identity — magic, class, data, `e_type`, `EM_X86_64` (`:524-550`) — program-header
bounds (`:557-579`), **every** PT_LOAD validated (`:581-626`), a short-read check on each segment copy, and the
magic re-checked at the load address (`:743-754`). Through 0.6.x the entire kernel-integrity check was ELF
magic, twice, and it loaded **one** program header. None of it is a hash or a signature.

- ⛔ **sigil is not on the boot path.** `gnoboot/cyrius.cyml` `[deps]` is `stdlib = ["fnptr"]` — one stdlib
  helper, nothing else. A case-insensitive grep for `verify|signature|ed25519|sha256|secureboot` across
  `gnoboot/src/` returns only two comments: a note to re-check `rdrand_u64` by disassembly after a toolchain bump (`main.cyr:285`) and a structural re-verify (`:743`). Whatever
  Authenticode surface sigil offers, no boot-path code calls it.
- 📋 **Secure Boot is post-v1.0**, explicitly not committed (`gnoboot/docs/development/roadmap.md:366-380`).
- 📋 **TPM / measured boot: absent**, also post-v1.0 (`roadmap.md:406-410`).
- ⚠️ **gnoboot's toolchain pin lag has narrowed**: `gnoboot/cyrius.cyml:7` pins `cyrius = "6.6.2"` against the
  6.6.6 toolchain (2026-09-27). When this plan was written it pinned 6.2.44 against 6.5.7 — a large lag on the
  one component with no exception handling and nothing between it and the firmware. Re-check it each pass.

⛔ **There is no address randomization in a shipped AGNOS.** Neither half of KASLR is live:
- The **image-base slide** requires an opt-in build flag nothing sets. `agnos/scripts/smoke/kaslr-smoke.sh:26-29`
  hard-errors *"build/agnos is ET_EXEC (non-PIE) — KASLR needs: `CYRIUS_PIE=1`"*. `CYRIUS_PIE` occurs **only**
  inside that smoke script's own usage text — zero occurrences elsewhere in `scripts/`, in agnos CI, in
  `sweep.sh`, or in the agnosticos ISO pipeline. `readelf -h build/agnos` on the 1.57.10 build → `Type: EXEC`,
  entry `0x1000a8`. gnoboot slides only `ET_DYN` (`gnoboot/src/main.cyr:644-671`).
- The **allocator half** is dead: `pmm_next_free` is RDRAND-seeded (`core/pmm.cyr:535-537`) and **no allocator
  consumes it as a start hint** — the bottom-up first-fit that did was removed at 1.41.12, and the comment at
  `pmm.cyr:543-561` says why (it scattered a 4 KB page into a random 2 MB region and made agnsh's ring-3 launch
  boot-flaky; `pmm_alloc` now runs top-down). The variable is still *read* — `pmm_free` lowers it at `pmm.cyr:598`
  and `:719`, and a boot diagnostic prints it (`core/main.cyr:524-526`) — but nothing allocates from it, so the
  randomness reaches no address a tester would have to guess.

**Consequence for the tester: every kernel gadget, `proc_table[]` and `vfs_table[]` address is at a
compile-time-known offset, derivable from the released ELF.**

⛔ **No Meltdown mitigation.** KPTI is **collapsed** — `syscall_hw.cyr:74-75`: *"KPTI is currently COLLAPSED
(kernel==user==running CR3) so the installs are same-value no-ops today."* `proc.cyr:1526-1545` records the same.
Kernel pages are never unmapped from the running address space. Architectural protection (U/S=0 + SMEP) holds;
speculative side-channel protection does not exist. ✅ IBRS is real but **CPUID-gated** — if the CPU does not
advertise it the instructions are not emitted at all, with no warning (`syscall_hw.cyr:403-414`). ⛔ No retpoline.

⛔ **DMA is unrestricted on the primary iron target.** `agnos/kernel/core/main.cyr:636` calls
`amd_iommu_disable()` **unconditionally**, writing AMD-Vi's Control Register to 0 — the in-source comment says
*"passthrough for everyone."* ⚠️ VT-d is Intel-only, and it **never enabled translation on any boot through
1.57.9** (the QEMU `q35` + `intel-iommu` boot printed `IOMMU: init failed`). Since **1.57.10** `iommu_init()` runs
every boot (`main.cyr:717-736`) and turns translation on when a drivable DMAR unit is present — gated in QEMU
only, **not burned**; a unit with VER ≥ 6 is refused. **archaemenid is AMD (Cezanne). On the iron AGNOS actually
runs on, any PCI device owns all of physical memory.**

### Tests
- [ ] BOOT-01 — Replace `\boot\agnos` on the ESP (a plain FAT partition) with a modified kernel; confirm it boots (**expected to succeed** — nothing verifies it)
- [ ] BOOT-02 — Derive kernel gadget addresses from the released ELF and confirm they match a running instance
- [ ] BOOT-03 — Malformed ELF against gnoboot's structural checks (a four-byte magic check through 0.6.x; size, identity, phdr-bounds and every-PT_LOAD validation since 0.7.0): oversized `p_memsz`, `p_paddr` overlapping firmware structures, `e_phoff` past EOF
- [ ] BOOT-04 — DMA attack over a PCIe or Thunderbolt-class device on AMD iron (⛔ IOMMU disabled)
- [ ] BOOT-05 — ⚠️ **EFER.NXE inheritance.** W^X depends on bit 63 meaning something. The Path C boot shim **never touches EFER** — its header says *"CR0/CR3/CR4/EFER — UEFI configured them"* (`boot_shim.cyr:221`). NX on the BSP is **inherited from firmware**, with no boot-time assertion (the APs set it themselves). On firmware that leaves NXE clear, every W^X mapping becomes a reserved-bit #PF. `proc.cyr:2390`'s parenthetical *"(EFER.NXE is set at boot — boot_shim.cyr)"* is **wrong for the shipping path**. Test on firmware that does not set it
- [ ] BOOT-06 — Meltdown / Spectre v1 against the collapsed KPTI on a vulnerable CPU

---

## Target 6 — Package supply chain

✅ **Verification happens at INSTALL, in ark — never at boot and never at exec.**
`ark/src/ark_package.cyr::ark_pkg_read` runs before any file is written: magic + format version, **SHA-256 root
hash**, **Ed25519 signature over the 32-byte root hash** (`:266-272`), bounded manifest/index/payload offsets,
a decompression-bomb cap, **per-file SHA-256** against the index, and a whole-data hash. A **zip-slip guard**
rejects the entire package before a single file lands (`:626-635`), and a symlink entry on a target without
symlink support fails closed (`:636-644`). (Lines as of ark 1.4.2.)

**Three gaps a tester must probe:**
- ⚠️ **The signature check is conditional.** It runs only if the `ARK_FLAG_SIGNED` bit is set. An unsigned
  package skips it entirely.
- ⚠️ **`require_signed` defaults to 0** — `ark/src/types.cyr:342` (`store64(p + 96, 0)`). Unsigned `.ark` files
  install; signed-by-anyone `.ark` files install. Only the explicit `--marketplace <URL>` path forces it on
  (`ark/src/marketplace.cyr:56-58`) — a marketplace step run from a resolved `--apply` plan installs through
  `ark/src/exec.cyr:176-200` under the default. **No config-file key, flag or environment variable sets it** —
  the policy is settable in code only.
- ⚠️ **Per-file hashes are skipped for an entry whose hash field is absent** (or not exactly 64 chars).

⛔ **There is no vulnerability-tracking mechanism for AGNOS dependencies.** No advisory DB, no CVE feed, no
OSV/NVD client, no scanner, in cyrius/cbt, ark, nous, takumi, zugot or mela. The nearest artifact,
`cyrius/programs/cyaudit.cyr`, is an **include-path policy checker**. ⚠️ `cyrius audit` is a *quality* sweep
(fmt/lint/docs/tests/bench) — the name is a trap; it scans no advisories.

✅ **What does work:** takumi enforces fetch → verify-SHA256 → *then* extract, in that order
(`takumi/src/cli.cyr:273-290`, *"never extract an unverified artifact"*), and the recipe parser **fails closed**
on a remote source whose `sha256` key is absent (`takumi/src/parse.cyr:203-218`). ⚠️ Not every zugot recipe is
pinned: some declare a remote source with `sha256 = ""` under the validator's `# TODO` carve-out — un-pinned
and unbuildable as written (see [vulnerability-management.md](vulnerability-management.md)). The recipes with no
`sha256` key at all are `local = true` virtuals with no upstream source.

⚠️ **Dep pinning is TOFU commit-pinning, and a `path =` override disables it.** `cyrius/cbt/deps.cyr:1884-1889`
says so in-source: a dep supplying both git/tag and an existing local `path` resolves from the path with no
clone, *"so the commit-pin verify below … is intentionally bypassed."* On a dev box with sibling checkouts the
tag is not a test. In CI the sibling does not exist, so the pin check runs — **CI is the first honest build of
the declared graph.** An untagged dep is never pinned; since cyrius 6.6.5 its cache is at least content-verified.

⛔ **The cyrius toolchain is the only signed release line found.** `cyrius/.github/workflows/release.yml:366-369`
refuses to publish without `CYRIUS_RELEASE_SK` (*"refusing to publish an UNSIGNED release"*) and then self-verifies
the signature against the committed `keys/cyrius-release.ed25519.pub` (`:376-377`) — fail-closed both ends.
⚠️ **Scope this negative to what was checked:** a 2026-09-27 sweep of every local repo's
`.github/workflows/*.yml` for `cyrsign` matched cyrius, plus two comments in samvada's workflows explaining why
it skips the install-time check; the release workflows of **ark, agnos, sigil, bote, takumi and nous** contain
**zero** signing steps (agnos, sigil, bote and nous publish an unsigned `SHA256SUMS`, ark and takumi none).
GitHub-only repos with no local checkout were not swept.

⚠️ **Every signature in the supply chain today is Ed25519.** ML-DSA-65 is compiled in by default and, as of
2026-09-27, is called from libro, argonaut's audit-log keygen and yantra's pin-descriptor verify (t-ron mentions
it only in comments). ark, takumi, nous, mela, bote, daimon, kavach and the kernel contain **no ML-DSA reference
at all**. "PQ default-on" means *the code is present*, not *anything is post-quantum signed*.

### Tests
- [ ] PKG-01 — Install an unsigned `.ark` on a default config (**expected to succeed** — `require_signed = 0`)
- [ ] PKG-02 — Install a package signed by an untrusted key with `require_signed = 0`
- [ ] PKG-03 — Ship a package entry with an absent per-file hash; confirm the content check is skipped
- [ ] PKG-04 — Zip-slip variants against `apkg_path_safe`
- [ ] PKG-05 — Dependency confusion via a repointed/force-pushed git tag; confirm the commit-pin refusal fires in CI and does **not** fire behind a `path =` override
- [ ] PKG-06 — Exploit the ⛔ absence of any advisory feed: pin a known-vulnerable upstream in a zugot recipe and confirm nothing flags it

### The claim that survives — do not "test" it as a weakness

✅ **CVE-2026-31431 structural immunity holds** (re-checked at agnos 1.57.10). There is no generic BSD socket
family, no `socket()` over arbitrary domains, no `splice`, and no `AF_ALG` — the net band is a fixed set of
purpose-built numbers over one kernel TCP/UDP stack with an 8-slot conn table, and **no kernel crypto is
reachable from ring 3 at all**. This survives as the syscall table grows, because it rests on *absent families*,
not on a version number or a count.
✅ Whole taxonomies have no code to attack: no `ptrace`, no `/proc`, no dynamic linker, no setuid bit, no kernel
modules, no eBPF, no io_uring, no copy-on-write (`fork`#96 exists since 1.56.54, as a full copy). The whole
syscall table fits in one document (`agnos/docs/development/agnos-userland-abi.md`) — attack-surface reduction is
the one metric where sovereignty pays immediately.

---

## Testing Methodology

### Phase 1: Reconnaissance
✅ External network scanning is still fully valid — the services listed above are real TCP listeners.
`nmap -sS -sV -O` and `nmap -sC` work as normal from a Linux attacker box. ⛔ Skip `testssl`: there is no TLS
terminator in the AGNOS network path; the kernel stack is plain TCP and every listed service speaks plain HTTP.

⭐ **The highest-yield recon is not network scanning — it is reading the kernel source.** It is a single Cyrius
binary with published sources; `syscall.cyr`'s dispatch arms are an exhaustive, commented list of every ring-3
entry point, and the comments name the gaps candidly.

### Phase 2: Vulnerability Assessment
Work Targets 1–6 above. Record for every finding **which target you tested** (AGNOS vs. Linux host build) —
a kavach or shakti finding on Linux says nothing about AGNOS.

### Phase 3: Exploitation
⛔ **Skip the privilege-escalation and sandbox-escape phases entirely** — see "Test plans that DO NOT APPLY."
Substitute:
- **Ring-3 → ring-0**: syscall-boundary memory corruption (Target 2, Target 3)
- **Process → process**: cross-owner shm, raw block read/write, and TCP-conn hijack on a kernel before 1.57.7 (Target 2)
- **Network → agent**: the unauthenticated daimon/bote chain (Target 4)
- **Disk → boot**: ESP kernel replacement (Target 5)
- **Device → memory**: DMA on AMD iron (Target 5)

### Phase 4: Post-Exploitation
- **Lateral movement** — cross-process shm is the real channel, and the 8-slot global conn table was one through
  1.57.6 (owned per process since 1.57.7). ⛔ There is no service-account pivot: there are no accounts.
- **Persistence** — the ESP (unverified kernel), the ark package DB, an installed `.ark` with no signature
  requirement. ⛔ No init-unit hijack in the systemd sense: PID 1 is kybernet.
- **Data exfiltration** — `blk_read #77` reads the entire disk ungated. ⚠️ Audit-log tampering is *detectable*
  (libro's hash chain) but not *preventable*.

## Testing Techniques
- **Black box** — no prior knowledge; external interfaces only. ⚠️ Low value here: the source is public, and
  most of the interesting surface is unauthenticated anyway.
- **White box** — full source access. ⭐ **The recommended mode.** The kernel comments name their own gaps.
- **Gray box** — limited documentation, standard user access. ⚠️ Note there *is* no "standard user" — every
  process is uid 0, so gray box collapses toward white box in practice.

---

## Test Case Index

⚠️ **No penetration test has ever been run against AGNOS.** Every Status cell below is empty because the work
has not happened, not because it passed.

| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| SVC-01 | daimon 8090 unauthenticated, network-reachable on AGNOS | Critical | |
| SVC-02 | hoosh auth fail-open with no tokens | High | |
| SVC-03 | bote tool invocation with no bearer | High | |
| SVC-04 | bote-bridge CORS `*` cross-origin invocation | High | |
| SVC-05 | kavach credential proxy secret disclosure | Critical | |
| SVC-06 | agora challenge replay / parked-login deadline | Medium | |
| SVC-07 | 8390/8391 port collision pre-emption | Medium | |
| SVC-09 | agnosai 8080 auth shipped but disabled by default | High | |
| SVC-10 | cyrius-yeomans-descent MUD pre-login surface + cleartext passphrase on 0.0.0.0:4000 | High | |
| SYS-01 | `is_user_range` boundary + wrap fuzz | Critical | |
| SYS-02 | High-arena pointer handling on every arm | High | |
| SYS-03 | TOCTOU inside the single open SMAP window | Critical | |
| SYS-04 | Cross-process shm read/write (`#72`/`#73`) | High | |
| SYS-05 | TCP conn hijack via another process's `conn_id` (regression test since 1.57.7) | High | |
| SYS-06 | `blk_rw_armed` magic arm → raw sector write | Critical | |
| SYS-07 | Memory-exhaustion DoS (uncapped by default; `spawn_limits` cap) | High | |
| SYS-09 | Reachable uninstalled exception vector → halt | High | |
| SYS-10 | 1.57.2–1.57.10 unburned kernel work (newest, least soak) | High | |
| SYS-11 | Kernel-stack overflow vs the 1.57.6 guard pages | Critical | |
| SYS-12 | Slab class mismatch / double free | High | |
| MEM-01 | Kernel table type confusion | Critical | |
| MEM-04 | Re-run race-adjacent tests under `-smp 4` | Critical | |
| AGENT-01 | Prompt injection hoosh → daimon agent | High | |
| AGENT-03 | Full unmitigated network → agent → tool chain | Critical | |
| AGENT-04 | Unsigned model checkpoint import | High | |
| AGENT-07 | libro audit-chain tampering (detect vs prevent) | Medium | |
| BOOT-01 | ESP kernel replacement (no verification) | Critical | |
| BOOT-02 | Gadget addresses from the released ELF | High | |
| BOOT-04 | DMA attack on AMD iron (IOMMU disabled) | Critical | |
| BOOT-05 | Firmware with EFER.NXE clear → W^X collapse | High | |
| PKG-01 | Unsigned `.ark` install on default config | High | |
| PKG-05 | Dep confusion via repointed tag / `path` override | High | |

---

## Tools

### Network
✅ nmap, netcat, wireshark, burp suite — all still apply against real TCP listeners.

### HTTP
✅ OWASP ZAP against daimon 8090, hoosh 8088, bote 8390–8393, vidya 8390, hadara 8391, mneme 8100, agnosai 8080.
✅ netcat/telnet against agora 2323 and cyrius-yeomans-descent 4000 (line-oriented, not HTTP — ZAP is the wrong tool).
⛔ **SQLMap and XSSer have no target** — there is no SQL anywhere in the stack and no browser-rendered surface.
(Removed from this list; they were fossils.)

### Binary analysis
✅ gdb, radare2, Ghidra — fully applicable to Cyrius-emitted ELF64. ⭐ `readelf -h build/agnos` confirming
`Type: EXEC` is itself the fastest proof that no address randomization is in play.

### Kernel
✅ QEMU with `-smp 4` for the SMP race work (MEM-04). ⚠️ QEMU does **not** reproduce every iron behaviour —
the `exit 142` (#PF kill) class on large binaries is iron-only. Confirm kernel findings on hardware.
✅ `agnos/scripts/smoke/` carries the real smoke battery, including the `syscall-harden-smoke.sh` runner that
landed 2026-08-05.

### AGNOS-native
✅ phylax one-shot `scan` (YARA rules, entropy, PE/ELF parsing) is the only phylax mode available on AGNOS —
`watch` fails closed (`phylax/src/cli.cyr:439-448`, no inotify) and daemon mode cannot run (no `AF_UNIX`).
⚠️ **phylax has zero consumers** — no repo declares `[deps.phylax]`. It is a standalone CLI you must invoke.
⚠️ **aegis is not a daemon.** `aegis/src/main.cyr` sets a log level (and, since 1.1.7, optionally opens an
`AEGIS_EVENT_LOG` sink), prints `"aegis ready"`, and exits. **Zero repos declare `[deps.aegis]`.** Its
"scanning" is four metadata checks (exists / size 0 / world-writable bit / > 500 MB) with ⛔ no content
inspection and no signature check — and on AGNOS even the metadata read fails, because the agnos stdlib's
`sys_fstat` is a stub returning −1 — and its "auto-quarantine" pushes onto an **in-memory** list without killing
or isolating anything. Do not treat it as a control under test.

---

## Reporting

### Vulnerability Format
```json
{
  "id": "AGNOS-2026-001",
  "title": "Cross-process shm read via shm_read #73",
  "severity": "High",
  "cvss": 7.5,
  "target": "agnos 1.57.10 kernel (NOT a Linux-host build)",
  "description": "...",
  "steps_to_reproduce": "...",
  "impact": "...",
  "remediation": "..."
}
```

⭐ **The `target` field is mandatory.** A finding against kavach's Linux backends, shakti, or phylax's inotify
path is a finding about Linux. Without that field a Linux-only result gets read as an AGNOS result — the exact
error that produced the previous version of this document.

⚠️ **Classify each finding against the posture, not against a Linux baseline.** "Any process can read another
process's shm" is a **documented design decision today** (`syscall.cyr:10284-10297`), not an implementation bug.
Report it as a gap in the posture with the source comment quoted, and let the maintainer decide. Reporting a
known, source-documented gap as a novel Critical wastes the pass.

### Severity Ratings
- **Critical**: Immediate risk of system compromise
- **High**: Significant impact, requires urgent fix
- **Medium**: Moderate impact, should be addressed
- **Low**: Minor issue, address when possible
- **Info**: Informational, no action required

## Timeline

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Planning | 1 day | Test plan, target-build declaration |
| Reconnaissance (incl. source read) | 3 days | Surface inventory |
| Vulnerability Assessment | 5 days | Vulnerabilities list |
| Exploitation | 3 days | Exploit proof-of-concepts |
| Reporting | 2 days | Final report |

## References
- ✅ `agnos/kernel/core/syscall.cyr` — the syscall dispatch table; with `agnos/docs/development/agnos-userland-abi.md`,
  the authoritative surface inventory
- ✅ `agnos/docs/development/security-hardening.md` — ⚠️ **read it with the code open, never on its own, and
  do not treat it as one vintage.** Its **S1–S13 table is old** — §8 says the set was *"closed at v1.28.0"* — and
  rows are invalidated by later code: **S7 (KASLR)** at `:249` still credits `kaslr_seed` randomizing
  `pmm_next_free` (dead since 1.41.12), **S8 (KPTI)** at `:250` claims *"Partial"* isolation (collapsed), and
  **S10** at `:252` says VT-d restricts DMA although VT-d translation never turned on through 1.57.9.
  Its **invariant 3** (`:17-20`, *"every legitimate user address is below 1 GB"*) predates the HIGH mmap arena,
  and §9 still lists "no per-process page budget" as open although 1.57.7 added the opt-in `spawn_limits`#107.
  But the file itself is **not** stale: §5 is the 1.50.7 process-isolation cut (2026-06-29), it cites 1.50.8, and
  §9's SMP note is the live warning MEM-04 rests on. Distrust the S-table; do not skip the document
- ✅ `agnos/docs/audit/2026-04-13-security-audit.md` — historical record against 1.21.0, not a posture statement
- ✅ `ark/docs/audit/2026-06-18-pre-v1-audit.md` — *"a signature without a trust anchor proves nothing about
  publisher identity"* (`:27-28`)
- ✅ `agnosticos/docs/development/planning/kavach-agnos-compat.md` — source-grounded audit of what kavach does
  and does not do on AGNOS
- 📋 `agnosticos/docs/development/planning/agent-injection-defense.md` — **roadmap, not a control**
- 📋 `gnoboot/docs/development/roadmap.md:366-380` — Secure Boot, explicitly post-v1.0 and uncommitted
- CVSS 3.x (severity scoring) · CVE / MITRE (identifier scheme)
- ⛔ **Removed**: kernel.org LSM / Landlock / seccomp-BPF references, and the CIS/OWASP-Linux control mappings.
  Per `docs/development/roadmap.md:160`, AGNOS confinement — *when* it lands — is capability-scoped and native,
  **never Landlock, seccomp or `unshare` ABI emulation**. A reference list is where a reader learns "how this
  works here"; pointing at mechanisms the project has permanently ruled out teaches the wrong system.
