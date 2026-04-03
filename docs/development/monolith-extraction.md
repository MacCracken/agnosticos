# Monolith Extraction Roadmap

> **Status**: COMPLETE | **Last Updated**: 2026-04-03
>
> All AGNOS core subsystems have been extracted from the monolithic userland workspace
> into independently buildable, updatable binaries with their own repos and ark packages.
> The monolith is fully dismantled as of 2026-04-01.

---

## Problem (solved)

All AGNOS core subsystems lived in a single Cargo workspace (`userland/`):

```
userland/
├── agent-runtime/     → daimon (port 8090) + argonaut + aegis + sigil + ark + nous + takumi + agnova + ...
├── llm-gateway/       → hoosh (port 8088)
├── ai-shell/          → agnoshi
├── desktop-environment/ → aethersafha
├── agnos-common/      → agnostik (shared types)
├── agnos-sys/         → agnosys (kernel interface)
└── agnos-sudo/        → shakti (privilege escalation)
```

This meant:
- **Any change to any subsystem required rebuilding the entire userland**
- **Updating daimon required a full OS update** — no independent package upgrades
- **agent-runtime was a megacrate** — daimon, argonaut, aegis, sigil, ark, nous, takumi, agnova, edge, federation, scheduler, and 20+ other modules all compiled into one binary
- **Consumer projects couldn't depend on subsystem crates** — sutra can't import argonaut's types, stiva can't import ark's signing logic, without depending on the entire agent-runtime

---

## Extraction Summary

### Phase 0 — Library Extractions (COMPLETE)

First wave — library crates with no HTTP server or service lifecycle:

| Crate | Extracted From | Version |
|-------|---------------|---------|
| kavach | agent-runtime sandbox | **2.0.0** (absorbed sandbox_mod) |
| majra | agent-runtime pubsub | Published |
| libro | agent-runtime audit | **0.90.0** (BLAKE3 support) |
| bote | agent-runtime MCP | **0.91.0** (absorbed MCP hosting + registry) |
| szal | agent-runtime workflow | Published |
| agnosai | agent-runtime orchestration | Published |
| ai-hwaccel | agent-runtime GPU detection | Published |

### Phase 1 — Types Foundation (COMPLETE)

| Crate | From | Version | Notes |
|-------|------|---------|-------|
| **agnostik** | agnos-common/ | **0.90.0** | Feature-gated (agent, security, telemetry, audit, llm, secrets, config, classification, validation, hardware). Git dep |
| **agnosys** | agnos-sys/ | **0.51.0** | 22 feature-gated modules, no openssl-sys. Git dep |
| **sigil** | agent-runtime/src/sigil.rs + integrity + trust | **1.0.0** | Owns ALL AGNOS crypto and trust. Released 2026-04-02 |

**Migration note (agnosys 0.51.0)**: The `agent` and `llm` features were removed. Consumer code that used `agnosys::agent::*` types should migrate to `agnosai`. Consumer code that used `agnosys::llm::*` types should migrate to `hoosh`. The `full` feature continues to work — it now enables all 22 system-level modules without pulling in `reqwest` or `openssl-sys`, which fixes aarch64 cross-compilation.

### Phase 2 — Package Management (COMPLETE)

| Crate | From | Version | Consumers |
|-------|------|---------|-----------|
| **ark** | agent-runtime/src/ark.rs | 0.1.0 | stiva, sutra, takumi, agnova, daimon |
| **nous** | agent-runtime/src/nous.rs | 0.1.0 | ark, daimon |
| **takumi** | agent-runtime/src/takumi.rs | 0.1.0 | ark, CI/CD, standalone CLI |

### Phase 3 — Init & Security (COMPLETE)

| Crate | From | Version | Type | Consumers |
|-------|------|---------|------|-----------|
| **argonaut** | agent-runtime/src/argonaut.rs | 0.90.0 | Library | stiva, sutra, daimon, kybernet |
| **kybernet** | Extracted from argonaut | 0.51.0 | Binary (PID 1) | OS boot |
| **aegis** | agent-runtime/src/aegis.rs | 0.1.0 | Binary | daimon, phylax, OS security |
| **agnova** | agent-runtime/src/agnova.rs | 0.1.0 | Binary | Installer (runs once) |
| **mela** | agent-runtime/marketplace/ | 0.1.0 | Service | Agent marketplace |
| **seema** | agent-runtime/edge/ | 0.1.0 | Service | Edge fleet management |
| **samay** | agent-runtime/scheduler.rs | 0.1.0 | Service | Task scheduler |

### Phase 3.5 — Crypto Boundary (RESOLVED)

**Decision**: Sigil owns all AGNOS crypto and trust (1.0.0 stable).

| Item | Resolution |
|------|-----------|
| **pqc.rs** | Future feature on sigil — no separate crate |
| **sy-crypto** | SY-side agent session crypto — separate from OS-level trust |
| **Ownership split** | AGNOS sigil = OS-level trust. SY sy-crypto = agent-side session crypto |
| **PQC timeline** | Post-v1.0 feature on sigil. Classical → hybrid → PQC-only transition planned |

### Phase 4 — Core Services (COMPLETE)

| Crate | From | Version | Notes |
|-------|------|---------|-------|
| **daimon** | agent-runtime/ | 0.6.0 | Standalone binary, leaned out, no transitive openssl-sys |
| **hoosh** | llm-gateway/ | 1.1.0 | Standalone binary (port 8088) |
| **agnoshi** | ai-shell/ | 0.90.0 | Standalone binary — 736 tests |
| **aethersafha** | desktop-environment/ | 0.1.0 | Standalone binary — 785 tests |
| **shakti** | agnos-sudo/ | 0.1.0 | Standalone binary — privilege escalation |

### Absorptions (existing crates grew)

| Target | Absorbed | New Version |
|--------|----------|-------------|
| **bote** | MCP hosting types + registry | 0.91.0 |
| **kavach** | sandbox_mod runtime modules | 2.0.0 |
| **t-ron** | safety module (injection, circuit breaker, policy) | 0.90.0 |

---

## Current State (2026-04-03)

**Monolith is fully dismantled.** All userland code lives in standalone repos under `/home/macro/Repos/{name}/`.

**Remaining in workspace**: `examples/` only (agent SDK examples, depends on agnostik + agnosys via git deps).

**Update flow** (the whole point of the extraction):
```
User or agent runs:  ark upgrade daimon
                         │
                         ▼
ark downloads:       daimon-2026.3.25-x86_64.ark
                         │
                         ▼
argonaut restarts:   daimon.service
                         │
                         ▼
Done.                No OS rebuild. No ISO. No reboot.
```

---

## What Daimon Became After Extraction

The agent-runtime megacrate (~50 modules) was broken down into:

```
daimon (the binary) owns ONLY:
  ├── HTTP API (port 8090)
  ├── Agent lifecycle (register, heartbeat, deregister)
  ├── Supervisor (agent process management)
  ├── IPC (Unix domain sockets)
  ├── Scheduler (task scheduling, cron)
  ├── Federation (multi-node clustering)
  ├── Edge fleet management
  ├── Memory store + vector store + RAG
  ├── MCP tool dispatch (delegates to bote)
  └── Screen capture/recording

daimon DEPENDS ON (as crate deps):
  ├── agnostik    — shared types
  ├── agnosys     — kernel bindings
  ├── kavach      — sandbox execution
  ├── majra       — pub/sub, queue, heartbeat
  ├── libro       — audit chain
  ├── bote        — MCP protocol
  ├── szal        — workflow engine
  ├── sigil       — trust verification
  ├── ark         — package operations
  ├── argonaut    — service management
  └── t-ron       — MCP security

Separately running services (not in daimon process):
  ├── hoosh       — LLM gateway (port 8088)
  ├── aegis       — security daemon
  ├── phylax      — threat scanner
  └── kybernet    — PID 1 (uses argonaut)
```

---

## Internal vs Public Boundary

Not everything extracted becomes a public crate. The question is: **"Would someone outside AGNOS use this?"**

### Internal (git dep, shipped via ark)

OS plumbing — runtime services and kernel bindings that only make sense as part of the AGNOS distribution:

| Crate | Why Internal |
|-------|-------------|
| **agnosys** | Kernel bindings (Landlock, seccomp, LUKS, TPM, IMA) — OS-specific |
| **agnostik** | OS-level shared types (sandbox configs, security policies, agent manifests) |
| **daimon** | Agent runtime service — the OS runtime |
| **agnoshi** | AI shell — the OS shell interface |

These ship as `.ark` packages via ark. Git dependencies for build-time consumption.

### Public (crates.io)

Domain libraries — reusable by anyone, not tied to the OS:

| Crate | Why Public |
|-------|-----------|
| **hoosh** | LLM inference gateway — LLM access is not an OS concern |
| **agnosai** | Agent orchestration types — agent primitives are domain-generic |
| **kavach** | Sandboxing library — generic enough for any Rust project |
| **libro** | Audit chain — generic cryptographic logging |
| **sigil** | Trust verification — crypto primitives are domain-generic |
| **bhava** | Emotion/personality engine — no OS dependency |
| **hisab**, **prakash**, etc. | Pure domain math/science — no OS dependency |

The OS provides the **runtime** (daimon). The **domain primitives** (agnosai, hoosh) are generic libraries that happen to be consumed by the OS. Same distinction as Linux shipping systemd (internal) while gstreamer (media) is a standalone project.

---

## Guiding Principles

1. **Extract libraries before services** — types and pure logic first, daemons last
2. **API compatibility** — external consumers (sutra, stiva, consumer apps) should not notice the extraction. HTTP API endpoints don't change. MCP tools don't change
3. **No flag day** — each extraction is independently releasable. The monolith shrinks incrementally
4. **Crate before binary** — extract the library crate first (types, logic), then the binary later. Argonaut-the-crate before kybernet-the-binary
5. **ark recipes drive the boundary** — if it has its own ark recipe and can be `ark upgrade`-d independently, it's properly extracted
6. **Internal vs public** — OS plumbing stays internal (git dep, ark package). Domain libraries go to crates.io. Ask: "Would someone outside AGNOS use this?"

---

---

*Last Updated: 2026-04-03*
