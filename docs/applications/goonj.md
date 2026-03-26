# Goonj

> **Goonj** (Hindi: echo/resonance) — Acoustics engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/goonj` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/goonj.toml` |
| crates.io | `goonj` |

---

## What It Does

- Sound propagation: ray-based and wave-based acoustic simulation
- Room acoustics: geometry-aware reverb, RT60 estimation, early reflections
- Impulse response generation for convolution reverb
- Material absorption coefficients and frequency-dependent attenuation
- Spatial audio: HRTF, ambisonics encoding, distance attenuation models

## Consumers

- **dhvani** — audio DSP (impulse responses for reverb effects)
- **kiran** — game engine (spatial audio, environmental acoustics)
- **joshua** — game simulation (realistic audio environments)
- **shruti** — DAW (room simulation for mixing)

## Architecture

- Built on hisab for geometry and wave math
- Ray tracing and image-source method for room simulation
- Dependencies: hisab, serde

## Roadmap

Stable at 1.0.0. Future: real-time occlusion/diffraction, outdoor propagation (wind, terrain), integration with pravash for aeroacoustics.
