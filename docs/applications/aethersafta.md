# Aethersafta

> **Aethersafta** (Greek aether + Arabic safha=surface) — Real-time media compositing engine

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.25.3` |
| Repository | `MacCracken/aethersafta` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/aethersafta.toml` |
| crates.io | [aethersafta](https://crates.io/crates/aethersafta) |

---

## What It Does

- Multi-source capture and compositing (screen, camera, overlay)
- Scene graph with layered composition and transform hierarchy
- Hardware-accelerated encoding via ai-hwaccel (GPU encode path)
- Real-time frame pipeline with configurable output formats
- Supports PNG, BMP, and raw pixel buffer outputs

## Consumers

- **aethersafha** — Desktop compositor (window compositing pipeline)
- **tazama** — Video editor (timeline rendering, export)
- Streaming and screen recording applications

## Architecture

- Scene graph model with render passes and blend operations
- Zero-copy frame handoff between capture and encode stages
- Dependencies: serde, tokio, ai-hwaccel, ranga

## Roadmap

Stable — published on crates.io. Future: HDR pipeline, multi-monitor capture, WebRTC output for remote streaming.
