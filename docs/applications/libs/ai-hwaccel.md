# ai-hwaccel

> **ai-hwaccel** (AI hardware acceleration) — Universal hardware accelerator detection and management

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.23.3` |
| Repository | `MacCracken/ai-hwaccel` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/ai-hwaccel.toml` |
| crates.io | [ai-hwaccel](https://crates.io/crates/ai-hwaccel) |

---

## What It Does

- Detects GPU, TPU, and NPU hardware across NVIDIA (CUDA), AMD (ROCm), Intel (oneAPI), and Apple (Metal) vendors
- Queries VRAM capacity, utilization, and compute capability per device
- Provides workload placement recommendations based on available accelerators
- Multi-device enumeration with unified API regardless of vendor
- Supports fallback to CPU when no accelerator is present

## Consumers

- **hoosh** — LLM inference gateway (model placement on GPU)
- **daimon** — Agent orchestrator (GPU-aware agent scheduling)
- **kiran** — Game engine (renderer device selection)
- All GPU-aware AGNOS applications

## Architecture

- Single crate with feature-gated vendor backends (cuda, rocm, oneapi, metal)
- Zero-cost abstractions: compile only the backends you need
- Dependencies: libc, serde

## Roadmap

Stable — published on crates.io. Future: runtime hot-swap between devices, power/thermal monitoring, NPU workload profiling.
