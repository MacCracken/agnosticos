# Stiva

> **Stiva** (Romanian: stack) — OCI container runtime

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/stiva` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/stiva.toml` |
| crates.io | `stiva` |

---

## What It Does

- OCI image management: pull, store, layer caching, content-addressable storage
- Container lifecycle: create, start, stop, exec, remove with resource limits
- Orchestration: multi-container composition, dependency ordering, health checks
- Networking: bridge, host, and container-to-container via nein integration
- Docker/Podman replacement purpose-built for AGNOS

## Consumers

- **daimon** — agent runtime (container-based agent execution)
- **sutra** — infrastructure orchestrator (container deployment tasks)
- Python and other non-native agent runtimes

## Architecture

- Built on kavach (sandbox backends) + majra (queue/heartbeat)
- OCI runtime spec compliant
- Dependencies: kavach, majra, serde, tokio

## Roadmap

Stable at 1.0.0. Future: rootless containers by default, image build (Dockerfile-compatible), registry mirroring, integration with edge fleet management.
