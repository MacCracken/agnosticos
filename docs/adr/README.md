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
| [101](adr-101-foundation-and-architecture.md) | Foundation and Architecture | Language (Rust), agent orchestration (daimon), LLM gateway (hoosh), cross-project integration, named subsystems |
| [102](adr-102-agent-runtime-and-lifecycle.md) | Agent Runtime and Lifecycle | Agent lifecycle, RAG pipeline, IPC, marketplace (mela), explainability, safety, fine-tuning, RL, federation, migration, scheduling |
| [103](adr-103-security-and-trust.md) | Security and Trust | Permission model, sandbox stack, sigil trust, aegis daemon, zero-trust hardening, post-quantum crypto, formal verification, novel sandboxing |
| [104](adr-104-distribution-build-and-installation.md) | Distribution, Build, and Installation | LFS-native distro, .ark package format, takumi build system, base system packages, argonaut init, agnova installer |
| [105](adr-105-desktop-environment.md) | Desktop Environment | Wayland compositor (aethersafha), accessibility, plugins, agent window ownership, clipboard, gestures |
| [106](adr-106-observability-and-operations.md) | Observability and Operations | OpenTelemetry, distributed tracing, Prometheus metrics, resource forecasting, audit chain, CI/CD |
| [107](adr-107-scale-collaboration-and-future.md) | Scale, Collaboration, and Future | Multi-node federation, cloud services, human-AI collaboration modes, research directions |
