# Kavach

> **Kavach** (Sanskrit: armor/shield) — Sandbox execution framework

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.1` |
| Repository | `MacCracken/kavach` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/kavach.toml` |
| crates.io | `kavach` |

---

## What It Does

- Backend abstraction: Native (Landlock/seccomp), gVisor, Firecracker, WASM, SGX, SEV
- Sandbox strength scoring for transparent security posture assessment
- Policy engine: declarative rules for filesystem, network, syscall access
- Credential proxy: secure secret injection without exposing to sandboxed code
- Lifecycle management: create, configure, start, stop, destroy with health checks

## Consumers

- **daimon** — agent runtime (all sandboxed agent execution)
- **stiva** — OCI container runtime (container isolation backend)
- **sutra** — infrastructure orchestrator (task sandbox profiles)
- Any AGNOS component that needs isolated execution

## Architecture

- Backend trait with pluggable implementations per isolation technology
- Strength scoring algorithm rates sandbox configurations 0-100
- Dependencies: serde, tokio, landlock, seccompiler

## Roadmap

Stable at 1.0.1. Core security primitive for the AGNOS ecosystem. Future: nested sandbox support, live migration between backends, TEE attestation improvements.
