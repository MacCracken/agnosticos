# Agnostik — Roadmap to 1.0.0

> **Status**: Planning | **Last Updated**: 2026-03-26
>
> Roadmap for hardening agnos-common into a production-grade `agnostik` internal crate.
> Follows the same pattern as agnosys 0.5.0 — lean out, harden, feature-gate.
> agnostik stays internal (path dependency) — it contains core OS types (sandbox, security, agent manifest) that do not belong on a public registry.
>
> **Blocked by this**: t-ron (MCP security monitor) needs agnostik types as a path dependency.

---

## Current State

| Metric | Value |
|--------|-------|
| Location | `userland/agnos-common/` |
| Lines | ~7,800 |
| Modules | 11 (agent, audit, config, error, llm, secrets, security, telemetry, types, lib core, security_tests) |
| Tests | 385 passing |
| Benchmarks | File exists, not populated |
| Feature gates | None — everything always compiled |
| `#[non_exhaustive]` | Missing on most enums |
| `#[must_use]` | Missing on pure functions |
| Dependencies | 18 (including reqwest, aes-gcm, tokio, rand, sha2, zeroize) |
| Distribution | Internal (path dep, shipped via ark) |

---

## Problems to Fix

### 1. Overlap with extracted crates

Same issue agnosys had — modules that now belong elsewhere:

| Module | Overlaps With | Action |
|--------|--------------|--------|
| `agent.rs` | **agnosai** (agent orchestration types) | Lean out — keep only the primitive types (AgentId, AgentStatus, AgentConfig, SandboxConfig). Move orchestration types to agnosai |
| `llm.rs` | **hoosh** (LLM types) | Remove — hoosh owns LLM types. agnostik should not have LLM provider types |
| `audit.rs` | **libro** (audit chain) | Evaluate — keep the AuditEntry/AuditChain primitives if they're genuinely shared, or defer to libro |

### 2. Too many dependencies for a types crate

agnostik is the **type vocabulary** — every crate in the stack depends on it. Heavy deps like `reqwest`, `tokio`, and `aes-gcm` mean every consumer pays that cost even if they only need types.

| Dependency | Why It's There | Action |
|-----------|---------------|--------|
| reqwest | Used somewhere in the crate | Remove — a types crate should not make HTTP calls |
| tokio | Telemetry uses async | Feature-gate behind `async` |
| aes-gcm | Secrets module encryption | Feature-gate behind `secrets` |
| rand | AgentId generation | Feature-gate or use uuid's rng |
| sha2 | Audit chain hashing | Feature-gate behind `audit` |
| zeroize | Secrets module | Feature-gate behind `secrets` |
| getrandom | Secrets module | Feature-gate behind `secrets` |
| subtle | Secrets module | Feature-gate behind `secrets` |
| once_cell | Config lazy init | Evaluate — may not be needed with LazyLock in std |
| num_cpus | Config | Evaluate — std::thread::available_parallelism exists |
| async-trait | Telemetry traits | Feature-gate behind `async` |

Target: **core agnostik with zero optional deps** — just serde, thiserror, uuid, chrono, tracing.

### 3. Missing hardening

| Issue | Fix |
|-------|-----|
| No `#[non_exhaustive]` on enums | Add to all public enums |
| No `#[must_use]` on pure fns | Add to all pure functions |
| No feature gates | Feature-gate every module except error + core types |
| No serde roundtrip test discipline | Ensure every public type has a roundtrip test |
| No benchmarks | Add criterion benchmarks for hot paths (serde, audit chain, telemetry) |
| Tests in lib.rs | Move to proper `#[cfg(test)] mod tests` per module |

---

## Phases

### Phase 0 — Lean Out (0.2.0)

Remove what doesn't belong. Same approach as agnosys 0.5.0.

- [ ] Remove `llm.rs` — hoosh owns these types
- [ ] Lean `agent.rs` — keep AgentId, AgentStatus, AgentType, AgentConfig, SandboxConfig, ResourceLimits, Permission. Move AgentEvent, AgentInfo, AgentStats to agnosai
- [ ] Evaluate `audit.rs` — keep if primitives are genuinely shared, otherwise defer to libro
- [ ] Remove `reqwest` dependency entirely
- [ ] Remove `once_cell` (use `std::sync::LazyLock`)
- [ ] Remove `num_cpus` (use `std::thread::available_parallelism`)
- [ ] Update all re-exports in lib.rs

**Migration notes**: Document what moved where, same as agnosys 0.5.0 did.

### Phase 1 — Feature Gates (0.3.0)

- [ ] Core (always on): error.rs, types.rs, core types from lib.rs (AgentId, UserId, AgentConfig, SandboxConfig, etc.)
- [ ] `secrets` feature: secrets.rs + aes-gcm, zeroize, getrandom, subtle, rand
- [ ] `audit` feature: audit.rs + sha2
- [ ] `security` feature: security.rs
- [ ] `telemetry` feature: telemetry.rs + tokio, async-trait
- [ ] `config` feature: config.rs
- [ ] `full` feature: all of the above
- [ ] `serde` feature: already implied but make explicit

**Cargo.toml target**:
```toml
[features]
default = ["serde"]
serde = ["dep:serde", "dep:serde_json", "uuid/serde", "chrono/serde"]
secrets = ["dep:aes-gcm", "dep:zeroize", "dep:getrandom", "dep:subtle", "dep:rand"]
audit = ["dep:sha2", "serde"]
security = ["serde"]
telemetry = ["dep:tokio", "dep:async-trait", "serde"]
config = ["serde"]
full = ["secrets", "audit", "security", "telemetry", "config"]
```

### Phase 2 — Harden (0.4.0)

- [ ] `#[non_exhaustive]` on all public enums
- [ ] `#[must_use]` on all pure functions
- [ ] Zero `unwrap()`/`panic!()` in library code
- [ ] Serde roundtrip tests for every public type
- [ ] Move inline tests from lib.rs into per-module test blocks
- [ ] Criterion benchmarks (serde roundtrip, audit chain, id generation)
- [ ] `cargo clippy --all-features --all-targets -- -D warnings` clean
- [ ] `cargo deny check` clean
- [ ] `cargo audit` clean

### Phase 3 — Release (1.0.0)

- [ ] Rename package to `agnostik` in Cargo.toml
- [ ] VERSION file: `1.0.0`
- [ ] CHANGELOG entry with migration guide from agnos-common
- [ ] Update workspace Cargo.toml path reference
- [ ] Update all internal consumers (daimon, hoosh, agnoshi, aegis, t-ron, etc.) to use path dep
- [ ] Stays internal — path dependency only, same as agnosys. Not published to crates.io

---

## Dependency Graph After 1.0.0

```
agnostik (types only, minimal deps)
    ├── serde, uuid, chrono, thiserror, tracing  (always)
    ├── aes-gcm, zeroize, rand                   (secrets feature)
    ├── sha2                                      (audit feature)
    └── tokio, async-trait                        (telemetry feature)

Internal consumers (path deps):
    agnosys ──→ agnostik
    daimon  ──→ agnostik
    hoosh   ──→ agnostik
    agnoshi ──→ agnostik
    t-ron   ──→ agnostik  (currently blocked)
    kavach  ──→ agnostik
    every consumer app ──→ agnostik (transitively via daimon/hoosh)
```

---

## v1.0 Criteria

- [ ] All enums `#[non_exhaustive]`
- [ ] All pure functions `#[must_use]`
- [ ] Zero unwrap/panic
- [ ] Feature-gated modules — core types compile with zero optional deps
- [ ] 100% serde roundtrip coverage on public types
- [ ] Criterion benchmarks with CSV history
- [ ] No overlap with agnosai, hoosh, or libro
- [ ] CI green on all platforms (x86_64, aarch64)
- [ ] t-ron unblocked and consuming agnostik via path dep

---

*Last Updated: 2026-03-26*
