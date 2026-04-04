# Cyrius Language Migration — Roadmap

> **Status**: Planning | **Last Updated**: 2026-04-04
>
> Migration strategy for converting AGNOS components from Rust to Cyrius.
> The bootstrap loop is closed (2026-04-04). The seed (Rust) can be retired.
> This roadmap defines the order, rationale, and criteria for each phase.
>
> **Principle**: Bottom-up migration following the dependency graph. Each phase
> must be fully proven before the next begins. Rust interop is maintained
> throughout — no big bang cutover.

---

## Prerequisites

- [x] cyrius-seed — zero-dependency assembler (102 tests, 13 MB/s pipeline)
- [x] stage1a through stage1f — incremental compiler bootstrap
- [x] **Bootstrap loop closed** — stage1f compiles asm.cyr, asm assembles stage1f, byte-exact match
- [x] Rust seed can be retired for forward progress
- [x] **Cyrius 1.0** — self-hosting compiler (1,467 lines, 43KB binary, 9ms self-compile, 41ms full bootstrap, zero external deps, 29KB auditable seed)
- [ ] Cyrius compiler handles structs, enums, generics, traits
- [ ] Cyrius compiler handles serde derive macros (or equivalent)
- [ ] Cyrius ↔ Rust FFI / interop layer proven
- [ ] Cyrius standard library bootstrapped (core types, collections, I/O)
- [ ] Cyrius package management via ark (not cargo)

---

## Phase 1 — Prove It (Types, No Runtime)

**Goal**: Prove Cyrius can handle the AGNOS type vocabulary. If agnostik compiles under Cyrius, the entire ecosystem can migrate incrementally.

| Crate | Version | Why First | Complexity |
|-------|---------|-----------|------------|
| **agnostik** | 0.90.0 | Every AGNOS component depends on it. Mostly types + serde. The cleanest test case. | Medium — 10 feature-gated modules, many types, serde derives |

**Success criteria**:
- agnostik compiles under Cyrius with identical public API
- All existing serde roundtrip tests pass
- Rust crates can depend on the Cyrius-compiled agnostik via interop
- No consumer changes required

**Why agnostik**: It's the shared vocabulary. Once the vocabulary compiles in the new language, every sentence built on it can start migrating. The roadmap already noted: "agnostik's types are the first things that must compile under Cyrius — every type shipped today is an implicit contract with the future compiler."

---

## Phase 2 — Pure Computation (No I/O, No Async)

**Goal**: Migrate crates that are pure types + logic. No filesystem, no network, no async runtime. These prove Cyrius handles real domain logic.

### 2A — New Infrastructure Crates

| Crate | Version | Domain | Complexity |
|-------|---------|--------|------------|
| **mudra** | 0.1.0 | Token/value primitives | Low — pure types |
| **vinimaya** | 0.1.0 | Transaction layer | Low — types + state machine |
| **taal** | 0.1.0 | Music theory | Low — pure types + math |
| **natya** | 0.1.0 | Drama/narrative | Low — pure types + enums |
| **kshetra** | 0.1.0 | Temporal geography | Medium — coordinates + temporal math |

### 2B — Science Crates (Pure Math)

| Crate | Version | Domain | Complexity |
|-------|---------|--------|------------|
| **impetus** | 1.3.0 | Physics | Low — math, no side effects |
| **kimiya** | 1.1.1 | Chemistry | Low |
| **hisab** | 1.4.0 | Higher math | Low |
| **pramana** | 1.2.0 | Statistics | Low |
| **sankhya** | 1.0.0 | Number systems | Low |
| **abaco** | 1.1.0 | Math engine | Low |
| **falak** | 1.0.0 | Orbital mechanics | Medium — numerical methods |
| **prakash** | 1.2.0 | Optics | Low |
| **pravash** | 1.2.0 | Fluid dynamics | Medium |
| **ushma** | 1.3.0 | Thermodynamics | Low |
| **pavan** | 1.1.0 | Aerodynamics | Medium |
| **bijli** | 1.1.0 | Electromagnetism | Medium |
| **tanmatra** | 1.2.1 | Atomic physics | Medium |
| **kana** | 1.1.0 | Quantum mechanics | Medium |
| (remaining science crates) | various | Various domains | Low–Medium |

### 2C — Audit/Record Crates

| Crate | Version | Domain | Complexity |
|-------|---------|--------|------------|
| **libro** | 0.92.0 | Audit chain (hash-linked logging) | Medium — SHA-256/BLAKE3, no network |

**Success criteria**:
- All crates compile under Cyrius
- All existing tests pass
- Benchmark parity or improvement vs Rust versions
- Rust consumers can use Cyrius-compiled versions via interop

---

## Phase 3 — System + Crypto (Unsafe Boundary)

**Goal**: Prove Cyrius can handle security-critical code and the kernel interface. This is where Cyrius meets `unsafe` and raw syscalls.

| Crate | Version | Domain | Complexity |
|-------|---------|--------|------------|
| **sigil** | 1.0.0 | Cryptographic trust (Ed25519, signing, verification) | High — crypto must be correct, timing-safe |
| **agnosys** | 0.51.0 | Kernel interface (Landlock, seccomp, syscalls) | High — unsafe FFI to Linux kernel |

**Success criteria**:
- sigil passes all 142 tests with identical cryptographic output
- agnosys syscall bindings work correctly on x86_64 and aarch64
- No timing side-channels introduced by the language change
- Security audit of the Cyrius-compiled versions

---

## Phase 4 — Language-Native Wins (Where Cyrius Adds Real Value)

**Goal**: Migrate crates where Cyrius language features provide capabilities Rust cannot express. These aren't just ports — they're **upgrades**.

| Crate | Version | Cyrius Advantage | Complexity |
|-------|---------|-----------------|------------|
| **kavach** | 2.0.0 | Sandbox-aware borrow checker — capabilities as types, sandbox boundaries enforced at compile time instead of runtime. The compiler rejects code that escapes its sandbox. | High |
| **bote** | 0.92.0 | Agent IPC as language primitives. Tool calls, JSON-RPC dispatch, message routing as first-class constructs, not library abstractions. | High |
| **t-ron** | 0.90.0 | Security policies as first-class types. Injection detection patterns, rate limit rules, circuit breaker policies expressible in the type system. | High |
| **nein** | 0.90.0 | Firewall rules as language constructs. Policy composition, NAT rules, port mapping as compile-time verified expressions. | Medium |
| **majra** | 1.0.4 | Queue/channel types as language primitives. Pub/sub patterns enforced by the compiler. | Medium |

**This phase is where Cyrius earns the migration.** Each crate should demonstrably do something the Rust version cannot:
- Compile-time sandbox escape prevention (kavach)
- Agent communication verified by the type system (bote)
- Security policy completeness checked at compile time (t-ron)

**Success criteria**:
- Each crate demonstrates at least one capability impossible in Rust
- Performance parity or improvement
- All existing tests pass plus new tests for language-native features
- ADR documenting what the Cyrius version can do that Rust cannot

---

## Phase 5 — The Brain (Async, Network, Orchestration)

**Goal**: Migrate the async/network layer. This requires Cyrius to have a mature async runtime and network stack.

| Crate | Version | Domain | Complexity |
|-------|---------|--------|------------|
| **daimon** | 0.6.0 | Agent orchestrator — HTTP API, IPC, lifecycle, MCP dispatch | Very High — async, networking, 144 MCP tools |
| **hoosh** | 1.2.0 | LLM gateway — 15 providers, token budgets, caching | Very High — async, HTTP clients, streaming |
| **nous** | 0.1.0 | Package resolver | Medium — graph algorithms + I/O |
| **ark** | 0.1.0 | Package manager CLI | Medium — CLI + filesystem |
| **takumi** | 0.1.0 | Build system | Medium — process spawning, filesystem |
| **szal** | 1.1.0 | Workflow engine | Medium — async, branching/retry |
| **mela** | 0.1.0 | Marketplace | Medium — HTTP, package registry |
| **seema** | 0.1.0 | Edge fleet | Medium — MQTT, fleet management |
| **phylax** | 0.5.0 | Threat detection | Medium — YARA, file scanning |
| **aegis** | 0.1.0 | Security daemon | Medium — system hardening |

**Prerequisites**:
- Cyrius async runtime (equivalent to tokio)
- Cyrius HTTP client/server (equivalent to reqwest/axum)
- Cyrius CLI framework (equivalent to clap)

**Success criteria**:
- Full feature parity with Rust versions
- Performance benchmarks within 5% or better
- All integration tests pass
- Boot time regression test (must not exceed current 3.2s desktop / 2.98s minimal)

---

## Phase 6 — The Interface (Last, Most Complex)

**Goal**: Migrate the user-facing layer. Last because it's the most complex and the least bottlenecked by language choice — it works fine in Rust.

| Crate | Version | Domain | Complexity |
|-------|---------|--------|------------|
| **aethersafha** | 0.1.0 | Wayland compositor | Very High — Wayland protocol, GPU, input handling |
| **agnoshi** | 0.90.0 | AI shell | High — natural language parsing, intent system |
| **kybernet** | 0.51.0 | PID 1 binary | Low code, but **most critical** — must be perfect |
| **argonaut** | 0.90.0 | Init system library | High — service management, boot sequencing |
| **shakti** | 0.1.0 | Privilege escalation | Medium — security critical |
| **agnova** | 0.1.0 | OS installer | Medium — disk operations, UI |
| **stiva** | 2.0.0 | Container runtime | High — OCI spec, overlay FS |

**kybernet note**: Smallest binary (2.2MB) but runs as PID 1. Last to migrate within this phase. The helmsman must be perfect.

**Success criteria**:
- Full feature parity
- Boot time at or below current benchmarks
- Desktop experience indistinguishable from Rust version
- Physical hardware validation (x86_64 + aarch64)

---

## Migration Principles

1. **Bottom-up, dependency order.** Never migrate a crate before its dependencies are proven.

2. **Interop throughout.** Rust and Cyrius crates coexist during the entire migration. No big bang cutover. A Cyrius crate can depend on a Rust crate and vice versa.

3. **Tests are the contract.** Every existing test must pass after migration. Tests are written in Rust against the public API — if the Cyrius version passes the same tests, the migration is valid.

4. **Benchmarks prove the migration.** Every crate gets before/after benchmarks. If Cyrius is slower, investigate before proceeding. The migration must be a win or neutral, never a regression.

5. **Phase 4 justifies the language.** Phases 1-3 prove Cyrius *can* replace Rust. Phase 4 proves it *should*. If Phase 4 doesn't deliver capabilities Rust cannot express, the migration loses its thesis.

6. **The consumer never notices.** External consumers of AGNOS crates (via crates.io or git deps) should see identical APIs. The language change is an implementation detail, not a breaking change.

---

## Relationship to Main Roadmap

| AGNOS Version | Cyrius Migration Phase |
|---------------|----------------------|
| v1.0 (Beta → Stable) | Phases 1-2 (types + pure computation in Cyrius) |
| v2.0 (Rust Kernel) | Phase 3 (agnosys in Cyrius, kernel interface proven) |
| v3.0 (Cyrius Language) | Phases 4-6 (full migration, cargo removed from toolchain) |
| v4.0 (Quantum Substrate) | Cyrius extensions for quantum primitives |

---

## Bootstrap Chain (Achieved 2026-04-04)

```
rustc 1.96.0-dev (built from source)
  → cyrius-seed (Rust assembler, 102 tests)
    → stage1a (compile-time codegen)
      → stage1b (runtime codegen, if/while/variables)
        → stage1c (extended operations)
          → stage1d (expanded capability)
            → stage1e (further extensions)
              → stage1f (self-hosting assembler)
                → asm.cyr (assembler in Cyrius's own language)
                  → stage1f_v2 (BYTE-EXACT MATCH)

The loop is closed. The seed is retired.
Every .cyr file in the chain produces identical output
whether assembled by the Rust seed or the Cyrius-native assembler.

Cyrius 1.0 (2026-04-04):
  Self-hosting compiler: 1,467 lines, 43KB binary
  Self-compile time:     9ms
  Full bootstrap:        41ms
  Seed binary:           29KB (smallest trusted binary in any self-hosting chain)
  External dependencies: Zero (no C, no Rust, no Python in any path)
  Total source:          6,560 lines (compiler + assembler + stages)
```

---

*Last Updated: 2026-04-04*
