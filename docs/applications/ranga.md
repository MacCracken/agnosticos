# Ranga

> **Ranga** (Sanskrit: color) — Core image processing library

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.24.3` |
| Repository | `MacCracken/ranga` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/ranga.toml` |
| crates.io | [ranga](https://crates.io/crates/ranga) |

---

## What It Does

- Color space conversions (sRGB, linear RGB, HSL, HSV, CMYK, CIE Lab)
- Blend modes (normal, multiply, screen, overlay, soft light, and more)
- Pixel buffer operations with generic pixel formats
- Image filters (blur, sharpen, edge detect, color correction)
- GPU-accelerated compute path via ai-hwaccel when available

## Consumers

- **rasa** — AI-native image editor (primary consumer)
- **selah** — Screenshot and annotation tool (image manipulation)
- **aethersafha** — Desktop compositor (compositing pipeline)

## Architecture

- Pure Rust core with optional GPU compute backend
- Generic over pixel type for zero-copy interop
- Dependencies: serde, ai-hwaccel (optional)

## Roadmap

Stable — published on crates.io. Future: HDR/wide-gamut support, SIMD-optimized filter kernels, ICC profile handling.
