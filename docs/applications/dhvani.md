# Dhvani

> **Dhvani** (Sanskrit: sound) — Core audio engine

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.22.4` |
| Repository | `MacCracken/dhvani` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/dhvani.toml` |
| crates.io | [dhvani](https://crates.io/crates/dhvani) |

---

## What It Does

- Audio buffer management with sample-accurate timing
- DSP primitives (filters, envelopes, oscillators, FFT)
- Sample rate conversion and format resampling
- Multi-channel mixing with per-channel gain and panning
- Audio analysis (spectrum, RMS, peak detection) and capture from system devices

## Consumers

- **shruti** — DAW (primary consumer, full DSP pipeline)
- **jalwa** — Media player (playback and EQ)
- **kiran** — Game engine (spatial audio, sound effects)
- **goonj** — Acoustics simulation backend

## Architecture

- Core audio buffer types with generic sample formats (f32, i16, i24)
- Lock-free ring buffers for real-time audio threads
- Dependencies: serde, symphonia (codec support)

## Roadmap

Stable — published on crates.io. Future: spatial audio (HRTF), VST3/CLAP plugin hosting bridge, PipeWire native backend.
