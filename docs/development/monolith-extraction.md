# Monolith Extraction Roadmap

> **Status**: Architectural Note | **Last Updated**: 2026-03-22
>
> Plan for extracting AGNOS core subsystems from the monolithic userland workspace
> into independently buildable, updatable binaries with their own ark packages.

---

## Problem

Today, all AGNOS core subsystems live in a single Cargo workspace (`userland/`):

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

This means:
- **Any change to any subsystem requires rebuilding the entire userland**
- **Updating daimon requires a full OS update** — no independent package upgrades
- **agent-runtime is a megacrate** — daimon, argonaut, aegis, sigil, ark, nous, takumi, agnova, edge, federation, scheduler, and 20+ other modules are all compiled into one binary
- **Consumer projects can't depend on subsystem crates** — sutra can't import argonaut's types, stiva can't import ark's signing logic, without depending on the entire agent-runtime

The shared crate extraction (kavach, majra, libro, bote, etc.) already solved this for *library* crates. This document addresses the remaining *service* binaries.

---

## Current State

### Already Extracted (crates.io)

These were the first wave — library crates with no HTTP server or service lifecycle:

| Crate | Extracted From | Status |
|-------|---------------|--------|
| kavach | agent-runtime sandbox | Published (0.22.3) |
| majra | agent-runtime pubsub | Published (0.22.3) |
| libro | agent-runtime audit | Published (0.22.3) |
| bote | agent-runtime MCP | Published (0.22.3) |
| szal | agent-runtime workflow | Published (0.21.3) |
| agnosai | agent-runtime orchestration | Published (0.21.3) |
| ai-hwaccel | agent-runtime GPU detection | Published (0.21.3) |

### Still Embedded (monolithic)

These are *service binaries* or *subsystem modules* that remain inside the userland workspace:

| Subsystem | Current Location | Binary? | Port | Why It Should Extract |
|-----------|-----------------|---------|------|----------------------|
| **daimon** | agent-runtime/ | Yes | 8090 | Core service — independent update cycle |
| **hoosh** | llm-gateway/ | Yes | 8088 | Core service — independent update cycle |
| **agnoshi** | ai-shell/ | Yes | — | Shell tool — update without OS rebuild |
| **aethersafha** | desktop-environment/ | Yes | — | Compositor — update without OS rebuild |
| **argonaut** | agent-runtime/src/argonaut.rs | Module | — | Init/service logic needed by stiva, sutra |
| **aegis** | agent-runtime/src/aegis.rs | Module | — | Security daemon — could run standalone |
| **sigil** | agent-runtime/src/sigil.rs | Module | — | Trust verification — library candidate |
| **ark** | agent-runtime/src/ark.rs | Module | — | Package management — needed by stiva |
| **nous** | agent-runtime/src/nous.rs | Module | — | Resolver — pairs with ark |
| **takumi** | agent-runtime/src/takumi.rs | Module | — | Build system — standalone CLI tool |
| **agnova** | agent-runtime/src/agnova.rs | Module | — | Installer — runs once, standalone |
| **phylax** | agent-runtime/src/phylax.rs | Module | — | Threat scanner — could run standalone |
| **agnostik** | agnos-common/ | Library | — | Shared types — already a crate, just not on crates.io |
| **agnosys** | agnos-sys/ | Library | — | Kernel bindings — already a crate, just not on crates.io |
| **shakti** | agnos-sudo/ | Yes | — | Small binary — low priority |

---

## Extraction Phases

### Phase 0 — Library Extractions (done)

Already completed. kavach, majra, libro, bote, szal, agnosai, ai-hwaccel published to crates.io.

### Phase 1 — Types Foundation

Extract the foundational type crates that everything else depends on. These have no service lifecycle — they're pure libraries.

| Extract | From | Becomes | Consumers |
|---------|------|---------|-----------|
| **agnostik** | agnos-common/ | `agnostik` crate (crates.io) | Every subsystem, every consumer app |
| **agnosys** | agnos-sys/ | `agnosys` crate (crates.io) | daimon, aegis, argonaut, stiva, kavach |
| **sigil** | agent-runtime/src/sigil.rs | `sigil` crate (crates.io) | ark, stiva, aegis, daimon |

**Why first**: These are leaf dependencies. Nothing downstream breaks. Consumer apps currently can't use AGNOS types without depending on the entire workspace — this fixes that.

### Phase 2 — Package Management

Extract the package management stack so stiva and sutra can depend on it directly:

| Extract | From | Becomes | Consumers |
|---------|------|---------|-----------|
| **ark** | agent-runtime/src/ark.rs | `ark` crate (crates.io) | stiva, sutra, takumi, agnova, daimon |
| **nous** | agent-runtime/src/nous.rs | `nous` crate (crates.io) | ark, daimon |
| **takumi** | agent-runtime/src/takumi.rs | `takumi` binary + crate | ark, CI/CD, standalone CLI |

**Why**: stiva needs ark's image signing/verification. sutra needs ark's install/upgrade logic. Today they go through daimon's HTTP API — fine for fleet ops, but local operations shouldn't require a running daimon.

### Phase 3 — Init & Security

Extract the subsystems that could run as independent daemons:

| Extract | From | Becomes | Consumers |
|---------|------|---------|-----------|
| **argonaut** | agent-runtime/src/argonaut.rs | `argonaut` binary + crate | stiva, sutra, daimon, OS boot |
| **aegis** | agent-runtime/src/aegis.rs | `aegis` binary | daimon, phylax, OS security |
| **phylax** | agent-runtime/src/phylax.rs | `phylax` binary | aegis, daimon, standalone scanning |
| **agnova** | agent-runtime/src/agnova.rs | `agnova` binary | Installer (runs once) |

**Why**: argonaut's service management logic is needed by stiva (container lifecycle) and sutra (service state modules). Today sutra shells out or uses daimon's API. With argonaut as a crate, sutra can call `argonaut::service::enable("tarang")` directly on the local machine without a daimon round-trip.

### Phase 4 — Core Services

The big extraction — daimon and hoosh become standalone binaries with their own repos, ark recipes, and independent release cycles:

| Extract | From | Becomes | Update Path |
|---------|------|---------|-------------|
| **daimon** | agent-runtime/ | `daimon` binary (ark package) | `ark upgrade daimon` or `sutra apply` |
| **hoosh** | llm-gateway/ | `hoosh` binary (ark package) | `ark upgrade hoosh` or `sutra apply` |
| **agnoshi** | ai-shell/ | `agnoshi` binary (ark package) | `ark upgrade agnoshi` |
| **aethersafha** | desktop-environment/ | `aethersafha` binary (ark package) | `ark upgrade aethersafha` |

**What remains in the monolith**: Nothing. The `userland/` workspace becomes a meta-package that depends on the extracted crates — or disappears entirely, replaced by individual repos.

**Update flow after extraction**:
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

## What Daimon Becomes After Extraction

Today `agent-runtime/` is ~50 modules compiled into one binary. After full extraction:

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
  └── t-ron       — MCP security (when ready)

Separately running services (not in daimon process):
  ├── hoosh       — LLM gateway (port 8088)
  ├── aegis       — security daemon
  ├── phylax      — threat scanner
  └── argonaut    — init system (PID 1 or systemd unit)
```

---

## Guiding Principles

1. **Extract libraries before services** — types and pure logic first, daemons last
2. **API compatibility** — external consumers (sutra, stiva, consumer apps) should not notice the extraction. HTTP API endpoints don't change. MCP tools don't change
3. **No flag day** — each extraction is independently releasable. The monolith shrinks incrementally
4. **Crate before binary** — extract the library crate first (types, logic), then the binary later. Argonaut-the-crate before argonaut-the-service
5. **ark recipes drive the boundary** — if it has its own ark recipe and can be `ark upgrade`-d independently, it's properly extracted

## Priority

- **Phase 1-2**: Pre-v1.0 — enables stiva and sutra to depend on ark/sigil/argonaut directly
- **Phase 3**: v1.0 — standalone security and init daemons
- **Phase 4**: Post-v1.0 — full independent update cycles for core services

---

*Last Updated: 2026-03-22*
