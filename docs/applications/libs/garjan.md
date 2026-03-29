# Garjan

> **Garjan** (Sanskrit: roar/thunder) — Environmental and nature sound synthesis

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/garjan` |
| Runtime | library crate (Rust) |

---

## What It Does

- Procedural synthesis of environmental sounds: weather, impacts, surfaces, fluids, fire, and ambient textures
- Physical-model generation — no samples, no assets, pure math
- Weather sounds: rain (intensity levels), thunder (distance-based), wind gusts
- Impact and contact sounds: footsteps, crashes, cracks with material properties (wood, metal, stone, earth)
- Fluid sounds: water flow, drips, splashes; fire: crackle, roar, hiss
- Continuous ambient textures: room tone, forest, city, ocean surf
- Optional naad backend for oscillators and filters
- `no_std` + `alloc` compatible (disable `std` feature)

## Consumers

- **kiran** — game engine (environmental audio, ambient soundscapes)
- **goonj** — acoustic simulation (environmental reverb sources)
- **dhvani** — audio engine (procedural ambient layers)
