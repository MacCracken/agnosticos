# Soorat

> **Soorat** (Arabic/Urdu: image/form, Farsi: face) — GPU rendering engine

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.24.3` |
| Repository | `MacCracken/soorat` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/soorat.toml` |
| crates.io | N/A (not yet published) |

---

## What It Does

- wgpu-based rendering abstraction (Vulkan, Metal, DX12 backends)
- Draw call batching and state management for efficient GPU submission
- Shader compilation and hot-reload (WGSL and SPIR-V)
- PBR (Physically Based Rendering) material pipeline with metallic-roughness workflow
- Render graph with automatic resource barrier management

## Consumers

- **kiran** — Game engine (primary consumer, all rendering)
- Future: any AGNOS app needing GPU-accelerated 2D/3D rendering

## Architecture

- Render graph with frame-level resource tracking
- Built on wgpu for cross-platform GPU access
- Uses hisab for math (vectors, matrices, transforms)
- Dependencies: wgpu, serde, hisab

## Roadmap

Pre-release — scaffolding phase. Future: shadow mapping, post-processing pipeline, instanced rendering, glTF scene import.
