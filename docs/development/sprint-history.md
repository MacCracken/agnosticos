# Sprint History

> Completed engineering backlog items and resolved sprint summaries.
> For full change details, see [CHANGELOG.md](/CHANGELOG.md).

---

## Completed Backlog Items

| # | Item | Completed |
|---|------|-----------|
| B3 | SHA256 checksums — all 264+ filled (100%) | 2026-03-18 |
| B4 | Debian debootstrap removal from build scripts | 2026-03-18 |
| E2 | MQTT bridge in daimon (`edge/mqtt_bridge.rs`, 14 tests) | 2026-03-18 |
| E3 | ESP32-CAM integration (13 tests) | 2026-03-18 |
| E4 | TinyML on ESP32-S3 (daimon side, 10 tests) | 2026-03-18 |
| S1 | gVisor/Firecracker runtime execution (`run_task()`, 47 tests) | 2026-03-18 |
| S3 | sy-agnos sandbox image Phase 1 (strength 80) | 2026-03-18 |
| S4 | sy-agnos dm-verity Phase 2 (strength 85) | 2026-03-18 |
| S5 | sy-agnos TPM measured boot Phase 3 (strength 88) | 2026-03-18 |
| T1 | Daimon remote exec API (10 tests) | 2026-03-18 |
| T2 | Daimon file transfer API (13 tests) | 2026-03-18 |
| T3 | Daimon playbook audit ingestion (5 tests) | 2026-03-18 |
| T4 | Hoosh playbook generation tuning | 2026-03-18 |
| T5 | sutra-community marketplace recipe | 2026-03-18 |
| H23 | File splitting (mcp_server, supervisor, wayland) | 2026-03-11 |
| H25 | Enum refactoring (MetricKind, BehaviorMetric, etc.) | 2026-03-11 |
| H26 | reqwest 0.11→0.12 upgrade (RUSTSEC-2025-0134) | 2026-03-14 |
| H27/H28 | Systemd Type=notify → Type=simple (boot fix) | 2026-03-14 |
| H29 | SSRF protection in HttpBridge | 2026-03-14 |
| H36 | Feature-gated desktop_environment behind `desktop` | 2026-03-14 |
| H37 | wasmtime 36→42 (WASI preview2 migration) | 2026-03-14 |

---

## Sprint Summaries

### 2026.3.22 (2026-03-19 to 2026-03-25)

| Category | Summary |
|----------|---------|
| Shared crate recipes | 37 new marketplace recipes (total 59). Every published crate now has a takumi recipe |
| Documentation | 15 new docs: app specs (impetus, joshua, muharrir, murti, stiva, t-ron, tanur), k8s-roadmap, monolith-extraction, network-evolution, science-crate-specs, shared-crates, AGNOS.md |
| Build pipeline | 10+ iterative ISO build fixes, selfhost-build.yml restructure, Rust MSRV 1.89 |
| Branding | Synapse → Irfan recipe rename |
| Refactor | llm-gateway `acceleration.rs` replaced with ai-hwaccel re-exports (−1128 lines) |

### 2026.3.20 (2026-03-19 to 2026-03-20)

| Category | Summary |
|----------|---------|
| Shared crates | 4 extracted: ai-hwaccel (crates.io), tarang (crates.io), aethersafta (scaffolded), hoosh (scaffolded) |
| ai-hwaccel | `acceleration.rs` replaced with re-exports (549 tests). Scheduler + finetune wired |
| ark-bundle | 23/23 bundles passing. 14 broken asset patterns fixed |
| Recipes | 10 created/updated |
| Release CI | tarang + ai-hwaccel multi-arch pipelines |

### 2026.3.18 (2026-03-18)

| Category | Summary |
|----------|---------|
| Sutra | v1 released — 5 crates, 70 tests, 6 MCP tools, SSH transport, sutra-community (5 modules) |
| Documentation | First-party standards, 18 app docs, roadmap split, os_long_term.md migrated |
| sy-agnos | All 3 phases complete (rootfs → dm-verity → TPM), strength 80 → 88 |
| ESP32 | Recipe + MQTT bridge + CAM + TinyML (daimon side) |
| Build | Debian debootstrap removed, SHA256 100% coverage |
| Synapse | All 7 bridge paths corrected, 21 handler tests |

### 2026.3.17 (2026-03-17 to 2026-03-18)

| Category | Summary |
|----------|---------|
| Refactoring | 10 module splits (~25K lines reorganized) |
| GPU | G1-G4: orchestrator, hoosh, edge, consumer GPU awareness |
| SY integration | 4 items: GPU status, local models, Firecracker passthrough, fleet heartbeat |
| Sandbox wiring | S1-S3: credential proxy, externalization gate, trust demotion |
| Agnostic | 13 integration items complete |
| Toolchain | Go 1.24 → 1.26 |

### 2026.3.16 (2026-03-16 to 2026-03-17)

| Category | Summary |
|----------|---------|
| Phase 16A | 9 desktop essential recipes (foot, helix, yazi, fuzzel, mako, zathura, imv, mpv, cliphist) |
| CI/CD | Two-tier build architecture (slow base rootfs + fast userland releases) |
| MCP tools | 106 → 122 (tarang + jalwa expansion) |

---

*This file is maintained alongside [CHANGELOG.md](/CHANGELOG.md). The changelog has full details; this file provides quick reference summaries.*
