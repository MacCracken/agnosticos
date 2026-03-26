# Muharrir — Shared Editor Primitives

> **Muharrir** (Arabic: محرر — editor/author) — reusable building blocks for AGNOS creative application editors

| Field | Value |
|-------|-------|
| Status | Published (0.23.5) |
| Priority | 3 — shared editor infrastructure for salai, rasa, tazama, shruti |
| Crate | `muharrir` (crates.io, available) |
| Repository | `MacCracken/muharrir` |
| Runtime | library crate |
| Domain | Editor UI primitives |

---

## Why First-Party

Every creative application in AGNOS — salai (game editor), rasa (image editor), tazama (video editor), shruti (audio DAW) — implements the same editor patterns independently: undo/redo stacks, property inspectors, hierarchy trees, expression evaluation in numeric fields, hardware-adaptive rendering quality.

Analysis of the four editors revealed:
- **Shruti** has the most mature undo system (1000-deep command history with apply/reverse)
- **Rasa** has explicit layer commands (AddLayer, RemoveLayer, etc.) with VecDeque eviction
- **Tazama** uses full-project snapshots for undo (Zustand store)
- **Salai** introduced libro-backed tamper-evident history

Muharrir extracts the common patterns into a single crate:
- **History** — libro audit chain undo/redo (salai pattern) with tamper-evident verification
- **Hierarchy** — generic parent-child tree builder that works for entities, layers, tracks, and buses
- **Inspector** — `PropertySheet` pattern for any editor panel showing object properties
- **Expression eval** — abaco-powered math input for any numeric field (`2*pi`, `sin(45)`)
- **Hardware detection** — ai-hwaccel quality tiers for adaptive rendering across all editors

## Design Principles

1. **Feature-gated** — consumers pull only what they need. A video editor that already has its own undo can use just `expr` + `hierarchy`.
2. **Domain-agnostic** — no game/image/audio/video assumptions. The hierarchy builder works with `NodeId` (u64), not entities or layers.
3. **Own the stack** — wraps AGNOS ecosystem crates (abaco, ai-hwaccel, libro, bhava), never external libs directly.
4. **Benchmark everything** — criterion benchmarks with CSV history tracking for every module.

## Architecture

### Where Muharrir Sits

```
salai (game editor)
  └── muharrir [full]          ← all features

rasa (image editor)
  └── muharrir [default]       ← history, hierarchy, inspector, expr, hw

tazama (video editor)
  └── muharrir [default]       ← history, hierarchy, inspector, expr, hw

shruti (audio DAW)
  └── muharrir [expr, inspector, hierarchy]  ← already has own undo
```

### Module Map

```
muharrir
├── hierarchy.rs   — generic tree builder (entities, layers, tracks, buses)
│                    build_hierarchy() + flatten() with closures for parent/name lookup
├── inspector.rs   — PropertySheet + Property types for editor panels
│                    categories, by_category(), serde support
├── history.rs     — undo/redo via libro::AuditChain [feature: history]
│                    cursor-based navigation, tamper-evident verify()
├── expr.rs        — math expressions via abaco::Evaluator [feature: expr]
│                    eval_f64(), eval_or(), eval_or_parse()
├── hw.rs          — hardware detection via ai_hwaccel [feature: hw]
│                    QualityTier (Low/Medium/High/Ultra), HardwareProfile
└── error.rs       — thiserror error types
```

### What Makes It Different

| Aspect | Per-App Implementation | Muharrir |
|--------|----------------------|----------|
| Undo/redo | 4 different implementations | Single libro-backed chain with tamper evidence |
| Expression input | None (plain number fields) | `2*pi`, `sin(45)`, `sqrt(2)/2` in any field |
| GPU quality | Manual configuration | Auto-detected quality tiers |
| Property panels | App-specific structs | Generic PropertySheet works everywhere |
| Tree views | App-specific tree logic | Generic build_hierarchy with closures |

## Dependencies

### AGNOS Shared Crates

| Crate | Feature | Purpose | Without It |
|-------|---------|---------|-----------|
| abaco | `expr` | Expression evaluation | Custom parser or no expression support |
| ai-hwaccel | `hw` | GPU/accelerator detection | Manual hardware probing per app |
| libro | `history` | Tamper-evident audit chain | Custom undo/redo stack per app |
| bhava | `personality` | NPC emotion/personality | Per-app personality systems |

### External Crates

| Crate | Purpose |
|-------|---------|
| serde | Serialization for Property, hierarchy nodes |
| thiserror | Error type derivation |
| tracing | Structured logging |

## Roadmap

### V0.1 — Scaffold (done, 2026-03-23)

- hierarchy, inspector, history, expr, hw modules
- Feature-gated architecture
- 44 tests, criterion benchmarks, example

### V0.2 — Command Pattern

- Generic `Command` trait with `apply()` / `reverse()` (inspired by shruti)
- Compound commands
- History integration with command trait

### V0.3 — Notifications

- Toast system with severity and auto-expiry
- Notification history for console panel

### V0.4 — Selection & State

- Generic selection tracker (single, multi, range)
- Panel visibility state management

### V1.0 — Production

- Stable API, domain-specific examples
- Published to crates.io

---

*Last Updated: 2026-03-23*
