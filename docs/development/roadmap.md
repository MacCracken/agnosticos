# AGNOS Development Roadmap

> **Status**: Pre-Alpha | **Last Updated**: 2026-03-10
> **All development phases complete** — 9240+ tests, ~82% coverage, 0 warnings
> **Next Milestone**: Alpha Release (Target: Q2 2026)

---

## Remaining Work for Alpha

### P1 - Alpha Blocker
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Third-party security audit | Security | 2 weeks | External | Vendor selection in progress |

### P2 - Alpha Polish
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Video tutorials | Documentation | 3 days | TBD | Not started |

### P3 - Beta/Post-Alpha
| Item | Component | Effort | Owner | Status |
|------|-----------|--------|-------|--------|
| Kernel Development Guide | Documentation | 3 days | TBD | Not started |
| Support portal | Infrastructure | 2 weeks | TBD | Not started |

---

## Phase Summary

All phases are complete. See [CHANGELOG.md](/CHANGELOG.md) for detailed implementation notes per version.

| Phase | Status | Tests | Key Deliverables |
|-------|--------|-------|------------------|
| 0-4 | Complete | — | Foundation through Desktop |
| 5 | Complete | — | Production hardening, code audits, CI/CD, module refactoring (http_api → 18 files, interpreter → 17 files) |
| 5.6 | Complete | — | All P0-P2 stubs eliminated |
| 6 | Complete | 200+ | Hardware acceleration, swarm intelligence, 32 networking tools, 7 agent wrappers |
| 6.5 | Complete | 550+ | 16 OS-level modules (FUSE, udev, PAM, bootloader, journald, IMA, TPM, etc.) |
| 6.6 | Complete | — | Consumer integration (Docker, WASM, security UI) |
| 6.7 | Complete | 100+ | Alpha polish (14 items: tab-completion, pipelines, aliases, KV store, dashboard, etc.) |
| 6.8 | Complete | 600+ | Beta features (34 items: RAG, RPC, OpenTelemetry, accessibility, anomaly detection, mTLS, etc.) |
| 7 | Complete | 199 | Federation (55), migration (54), scheduling (47), ratings (43) |
| 8A-8F | Complete | 205 | Distribution: sigil (35), takumi (43), argonaut (46), agnova (41), aegis (40) |
| 8G | Complete | 68 | Post-quantum cryptography |
| 8H-8J | Complete | 209 | Explainability (59), AI safety (77), fine-tuning (73) |
| 8K-8M | Complete | 221 | Formal verification (76), novel sandboxing (77), RL optimization (68) |
| 9 | Complete | 169 | Cloud services (82), human-AI collaboration (87) |

---

## Release Roadmap

### Alpha Release — Q2 2026

**Current version**: `2026.3.10` (CalVer: `YYYY.M.D`, patches as `-N`)

**Remaining criteria:**
- [ ] Third-party security audit complete

**Target Date**: End of Q2 2026

### Beta Release — Q3 2026

**Remaining:**
- [ ] Community testing program
- [ ] Support channels open (Discord, forum)
- [ ] Video tutorials published

**Target Date**: Mid-Q3 2026

### v1.0 Release — Q4 2026

**Criteria:**
- Production ready (all critical bugs resolved)
- Enterprise features complete (SSO, audit logging, mTLS)
- Commercial support available
- Migration guides published
- Marketplace consumer apps published to mela

---

## Future Work (Post-Alpha, Demand-Gated)

### Web Browser

**Phase 1 — Browser Suite (Alpha)** `recipes/browser/`
- [ ] Build and package all 8 browsers as `.ark`

**Phase 2 — AI-Integrated WebView (Proposed, Post-Beta)**
- [ ] Lightweight embedded browser using `wry`/`tauri` WebView
- [ ] AI features: page summarization, agent-assisted form filling, smart bookmarks
- [ ] Deep integration with hoosh (LLM gateway) for on-device inference
- [ ] Privacy-first: all AI processing local, no cloud telemetry

**Phase 3 — Custom Browser Shell (Proposed, Post-v1.0)**
- [ ] Thin shell around Servo or Chromium Embedded Framework (CEF)
- [ ] Native aethersafha compositor integration (no intermediate toolkit)
- [ ] Agent-driven browsing: natural language navigation, automated workflows
- [ ] Sandboxed per-tab via AGNOS agent runtime (each tab = sandboxed agent)

### Python Runtime & Version Management

Native Python support via ark/takumi/nous — no external version manager dependency.
Borrows conventions from pyenv (`.python-version` files) and mise (hook-env pattern).

**Phase 1 — CPython as ark packages** `recipes/python/`
- [ ] Build CPython `.ark` packages on native target (bare-metal / VM install)
- [ ] Build CPython `.ark` packages in container (takumi builder container)
- [ ] Verify shared lib coexistence with multiple installed versions on both targets
- [ ] Update `docker/Dockerfile.python` to use ark-built CPython instead of upstream `python:3.12-slim-bookworm`

**Phase 2 — Version switching**
- [ ] Rust shim binary (`/usr/bin/python` → resolves version via hook-env)
- [ ] `.python-version` file support (project-level, compatible with pyenv/mise/uv)
- [ ] Agent runtime integration: auto-select Python version from agent metadata `"runtime": "python", "version": "3.12"`
- [ ] `ark python list` / `ark python use 3.13` CLI commands

**Phase 3 — Virtual environment integration**
- [ ] `ark venv create` — thin wrapper around `python -m venv` with audit logging
- [ ] Per-agent venv isolation (auto-created in agent sandbox)
- [ ] Seccomp profile already exists (`SeccompProfile::Python`, ~45 syscalls)

**Phase 4 — Package management hooks (post-v1.0)**
- [ ] `ark pip install` — pip proxy with sigil signature verification for wheels
- [ ] Curated `.ark` packages for common Python libs (numpy, requests, etc.)
- [ ] Optional uv integration as accelerated resolver backend

### Docker Base Images

Runtime-specific base images for consumer projects. Published to `ghcr.io/maccracken/agnosticos:<tag>` on each release.

| Image | Dockerfile | Status |
|-------|-----------|--------|
| `agnosticos:node22` | `Dockerfile.node` (Node.js 22 + Bun) | CI ready |
| `agnosticos:python3.13` | `Dockerfile.python3.13` | CI ready |
| `agnosticos:python3.14` | `Dockerfile.python3.14` (RC) | CI ready |
| `agnosticos:rust` | `Dockerfile.rust` | CI ready |

- [ ] `agnosticos:python3.13t` — Free-threaded Python 3.13 (GIL-disabled, needs separate Dockerfile)

### Marketplace Consumer Apps `recipes/marketplace/`

Bundles are built automatically from GitHub releases via `ark-bundle.sh` — no local repos needed.

All 5 consumer apps have marketplace recipes + GitHub release bundling via `ark-bundle.sh`.

**SecureYeoman** (Bun, ~42MB) | **BullShift** (Rust, ~2.8MB) | **Photis Nadi** (Flutter, ~20MB) | **Agnostic** (Python, ~472KB) | **Synapse** (Rust, pending first release)

- [ ] Publish all to mela marketplace
- [ ] Photis Nadi: MCP agent bridge integration (planned AGNOS desktop feature)

### Federation Enhancements

- [ ] Shared vector store across federated nodes

### Full Convergence (Demand-Gated)

- [ ] **Unified SSO/OIDC provider** — AGNOS as OIDC-aware service
- [ ] **Cross-project agent delegation** — External orchestrator → A2A → AGNOS sandbox
- [ ] **Shared vector store federation** — AGNOS embedded vector store queryable via REST
- [ ] **Unified agent marketplace backend** — AGNOS registry as single source of truth

### Additional Post-v1.0

- [ ] gRPC API (alongside REST)
- [ ] Service mesh readiness (Envoy/Linkerd sidecar injection)

---

## Key Performance Indicators (KPIs)

### Current Status (as of 2026-03-10)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Code Coverage | >80% | ~82% | Met |
| Test Pass Rate | 100% | 100% | Met |
| Total Tests | 400+ | 9240+ | Met |
| Agent Spawn Time | <500ms | ~300ms | Met |
| Shell Response Time | <100ms | ~50ms | Met |
| Memory Overhead | <2GB | ~1.2GB | Met |
| Boot Time | <10s | N/A | Pending |
| CIS Compliance | >80% | ~85% | Met |
| Stub Implementations | 0 | 0 | Met |
| Compiler Warnings | 0 | 0 | Met |

### By Component

| Component | Tests | Notes |
|-----------|-------|-------|
| agnos-common | 307 | Secrets, telemetry, LLM types, manifest, rate limits, audit chain |
| agnos-sys | 750+ | 16 modules: audit, mac, netns, dmverity, luks, ima, tpm, secureboot, certpin, bootloader, journald, udev, fuse, pam, update, llm |
| agent-runtime | 2763+ | Orchestrator, IPC, sandbox, registry, marketplace (88+43), federation (55), migration (54), scheduler (51), PQC (68), explainability (59), safety (77), finetune (73), formal_verify (76), sandbox_v2 (79), rl_optimizer (68), cloud (82), collaboration (87), sigil (46), aegis (55), takumi (57), argonaut (78), agnova (55), database (42) |
| llm-gateway | 710 | 15 providers (5 native + 10 OpenAI-compatible), rate limiting, streaming, cert pinning, hardware acceleration, token budgets |
| ai-shell | 1132 | 25+ intents, approval workflow, dashboard, aliases, completion |
| desktop-environment | 1447+ | Wayland protocol (63+49), screen capture (31), screen recording (22+), plugin host (31), xwayland (20), shell integration (26), theme bridge (18), compositor, renderer |

---

## Architecture Decision Records

| # | ADR | Status |
|---|-----|--------|
| 001 | Foundation and Architecture | Accepted |
| 002 | Agent Runtime and Lifecycle | Accepted |
| 003 | Security and Trust | Accepted |
| 004 | Distribution, Build, and Installation | Accepted |
| 005 | Desktop Environment | Accepted |
| 006 | Observability and Operations | Accepted |
| 007 | Scale, Collaboration, and Future | Accepted |

---

## Named Subsystems

| Name | Role | Component |
|------|------|-----------|
| **hoosh** | LLM inference gateway (port 8088, 15 providers) | `llm-gateway/` |
| **daimon** | Agent orchestrator (port 8090) | `agent-runtime/` |
| **agnosys** | Kernel interface | `agnos-sys/` |
| **agnostik** | Shared types library | `agnos-common/` |
| **shakti** | Privilege escalation | `agnos-sudo/` |
| **agnoshi** | AI shell (`agnsh`) | `ai-shell/` |
| **aethersafha** | Desktop compositor | `desktop-environment/` |
| **ark** | Unified package manager | `ark.rs`, `/v1/ark/*` |
| **nous** | Package resolver daemon | `nous.rs` |
| **takumi** | Package build system | `takumi.rs` |
| **mela** | Agent marketplace | `marketplace/` module |
| **aegis** | System security daemon | `aegis.rs` |
| **sigil** | Trust verification | `sigil.rs` |
| **argonaut** | Init system | `argonaut.rs` |
| **agnova** | OS installer | `agnova.rs` |
| **vansh** | Voice AI shell (planned) | TBD |

---

## Contributing

### Priority Contribution Areas

1. **Third-party security audit (P1)** — External vendor engagement
2. **Video tutorials (P2)** — Installation, usage, agent creation, security overview
3. **Kernel Development Guide (P3)** — For kernel hackers contributing to AGNOS kernel modules
4. **Support portal (P3)** — Community support channels

### Getting Started

See [CONTRIBUTING.md](/CONTRIBUTING.md) for:
- Development environment setup
- Code style and testing requirements
- Git workflow and commit conventions
- Pull request process

---

## Resources

- **Repository**: https://github.com/agnostos/agnos
- **Documentation**: https://docs.agnos.org (planned)
- **Issue Tracker**: https://github.com/agnostos/agnos/issues
- **Changelog**: [CHANGELOG.md](/CHANGELOG.md)

---

*Last Updated: 2026-03-10 | Next Review: 2026-03-17*
