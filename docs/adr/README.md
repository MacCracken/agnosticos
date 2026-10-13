# Architecture Decision Records

## ADR Format

Each ADR follows this structure:

- **Status** — Accepted, Proposed, Deprecated, or Superseded
- **Date** — When the decision was made
- **Context** — What problem we're solving
- **Decisions** — What we chose and why
- **Consequences** — Trade-offs (positive and negative)

## ADR Index

| ADR | Title | Scope |
|-----|-------|-------|
| [001](adr-001-foundation-and-architecture.md) | Foundation and Architecture | ⚠ Partially superseded by ADR-008. Language (Rust → Cyrius per ADR-008), agent orchestration (daimon), LLM gateway (hoosh), cross-project integration, named subsystems |
| [002](adr-002-agent-runtime-and-lifecycle.md) | Agent Runtime and Lifecycle | Agent lifecycle, RAG pipeline, IPC, marketplace (mela), explainability, safety, fine-tuning, RL, federation, migration, scheduling |
| [003](adr-003-security-and-trust.md) | Security and Trust | Permission model, sandbox stack, sigil trust, aegis daemon, zero-trust hardening, post-quantum crypto, formal verification, novel sandboxing |
| [004](adr-004-distribution-build-and-installation.md) | Distribution, Build, and Installation | LFS-native distro, .ark package format, takumi build system, base system packages, argonaut init, agnova installer |
| [005](adr-005-desktop-environment.md) | Desktop Environment | ⚠ Display protocol superseded 2026-07-06 — **Wayland refused**, native protocol (see [design-patterns.md](../design-patterns.md) + aethersafha ADR 0001). Accessibility, plugins, agent window ownership, clipboard, gestures remain. |
| [006](adr-006-observability-and-operations.md) | Observability and Operations | OpenTelemetry, distributed tracing, Prometheus metrics, resource forecasting, audit chain, CI/CD |
| [007](adr-007-scale-collaboration-and-future.md) | Scale, Collaboration, and Future | Multi-node federation, cloud services, human-AI collaboration modes, research directions |
| [008](adr-008-cyrius-as-sovereign-systems-language.md) | Cyrius as Sovereign Systems Language | Pivot from Rust to Cyrius (2026-04-04). Registry sovereignty, hand-auditable bootstrap, full-stack ownership. Partially supersedes ADR-001 (language only). |
