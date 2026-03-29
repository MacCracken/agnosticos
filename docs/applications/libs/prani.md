# Prani

> **Prani** (Sanskrit: living being/creature) — Creature and animal vocal synthesis

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/prani` |
| Runtime | library crate (Rust) |

---

## What It Does

- Vocal synthesis for non-human creatures: animals, fantasy beings, alien species
- Species-specific vocal tract models with individual variation
- Call pattern generators: growl, chirp, howl, roar, hiss, purr
- Behavioral vocalization mapping via CallIntent: alarm, territorial, mating, pain, idle
- Built on svara's glottal source and formant synthesis engine
- Emotion-driven vocal modulation
- Vocal fatigue simulation
- Preset species library (wolf, bird, cat, etc.)
- Optional C FFI buffer-callback API for game middleware integration
- Optional naad backend for high-quality DSP
- `no_std` + `alloc` compatible

## Consumers

- **kiran** — game engine (creature/animal audio, NPC vocalizations)
- **dhvani** — audio engine (creature vocal layers)
- **joshua** — game manager (AI NPC voice generation)
