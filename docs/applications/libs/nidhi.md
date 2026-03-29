# Nidhi

> **Nidhi** (Sanskrit: treasure/storehouse) — Sample playback engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/nidhi` |
| Runtime | library crate (Rust) |

---

## What It Does

- Sample-based instrument playback with key/velocity zone mapping
- Polyphonic sampler engine with configurable voice pool and voice stealing
- Loop modes, time-stretching, and interpolation
- SFZ and SF2 instrument format import
- Sample bank management (mono and stereo f32 waveforms)
- Per-zone root note, tuning, and loop point configuration
- ADSR envelope per voice
- Denormal flushing for safe feedback-loop processing on x86
- Optional I/O via shravan (WAV/PCM codec integration)
- Optional SIMD acceleration
- `no_std` + `alloc` compatible (disable `std` feature)

## Consumers

- **shruti** — DAW (sampler instrument tracks)
- **dhvani** — audio engine (sample playback layer)
- **kiran** — game engine (sound effect playback)
