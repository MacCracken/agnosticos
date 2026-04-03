# Agnostik — Roadmap to 1.0.0

> **Status**: Active — 0.90.0 released | **Last Updated**: 2026-04-03
>
> Roadmap for hardening agnostik into a production-grade 1.0.0 release.
> agnostik is the shared type vocabulary for all AGNOS components.
> Standalone repo: `MacCracken/agnostik` | Distribution: git dep (not on crates.io due to name collision)
>
> **Feature-gated**: agent, security, telemetry, audit, llm, secrets, config, classification, validation, hardware

---

## Current State (0.90.0)

| Metric | Value |
|--------|-------|
| Location | Standalone repo (`MacCracken/agnostik`) |
| Version | 0.90.0 |
| Distribution | Git dep (`version` + `git` spec) |
| Feature gates | 10 (agent, security, telemetry, audit, llm, secrets, config, classification, validation, hardware) |
| Consumers | Every AGNOS component: daimon, hoosh, agnoshi, aegis, argonaut, sigil, ark, kavach, stiva, nein, and all consumer apps |

### Completed (Phases 0–1)

- [x] Extracted to standalone repo
- [x] Removed module overlaps with agnosai, hoosh, libro
- [x] Feature-gated all modules — consumers pull only what they need
- [x] Leaned out dependency tree
- [x] Tagged 0.90.0

---

## Remaining Work to 1.0.0

### Hardening (0.90.0 → 1.0.0)

- [ ] `#[non_exhaustive]` on ALL public enums
- [ ] `#[must_use]` on all pure functions
- [ ] Zero `unwrap()`/`panic!()` in library code
- [ ] Serde roundtrip tests for every public type
- [ ] Criterion benchmarks (serde roundtrip, id generation, hot paths)
- [ ] `cargo clippy --all-features --all-targets -- -D warnings` clean
- [ ] `cargo deny check` clean
- [ ] `cargo audit` clean
- [ ] CI green on all platforms (x86_64, aarch64)

### Documentation

- [ ] CHANGELOG entry with migration guide from agnos-common
- [ ] Per-module doc comments
- [ ] Usage examples in doc tests

---

## Dependency Graph

```
agnostik (types only, minimal deps)
    ├── serde, uuid, chrono, thiserror, tracing  (always)
    ├── feature-gated optional deps per module
    └── zero heavy deps in default feature set

Internal consumers (git deps):
    agnosys ──→ agnostik
    daimon  ──→ agnostik
    hoosh   ──→ agnostik
    agnoshi ──→ agnostik
    t-ron   ──→ agnostik
    kavach  ──→ agnostik
    sigil   ──→ agnostik
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

---

*Last Updated: 2026-04-03*
