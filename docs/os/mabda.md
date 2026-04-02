# Mabda

> **Mabda** (Arabic: origin/principle) — GPU foundation layer

| Field | Value |
|-------|-------|
| Status | Pre-1.0 |
| Version | `0.1.0` |
| Repository | `MacCracken/mabda` |
| Runtime | library crate (Rust) |

---

## What It Does

- Shared GPU foundation for all AGNOS GPU consumers, owns the wgpu dependency
- Device lifecycle management (GpuContext: adapter, device, queue)
- Buffer management: storage, uniform, and staging buffers with read-back
- Compute pipeline: shader dispatch utilities for GPGPU workloads
- Texture handling: creation, caching, default sampler, format support (PNG, JPEG)
- Render targets for offscreen and onscreen rendering
- GPU capability detection and feature querying
- Frame profiler with GPU timestamp queries and pass timing
- Color type with common presets
- Feature-gated: `graphics` (textures, render targets), `compute` (pipelines, storage buffers), `full` (both)

## Consumers

- **soorat** — rendering engine (sprites, PBR, shadows, post-fx)
- **rasa** — image editor (GPU compute filters)
- **ranga** — image processing library (GPU pixel ops)
- **bijli** — electromagnetic simulation (FDTD compute)
- **aethersafta** — desktop compositor (GPU compositing)
- **kiran** — game engine (via soorat)
