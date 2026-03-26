# Prakash

> **Prakash** (Sanskrit: light/illumination) — Optics and light simulation

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/prakash` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/prakash.toml` |
| crates.io | `prakash` |

---

## What It Does

- Ray optics: tracing, reflection, refraction, Fresnel equations, caustics
- Wave optics: interference, diffraction, polarization, thin-film effects
- Spectral math: wavelength/frequency conversion, blackbody radiation, CIE color matching
- Lens geometry: thick/thin lenses, aberrations, multi-element optical systems
- PBR materials and atmospheric scattering (Rayleigh, Mie)

## Consumers

- **soorat** — rendering engine (PBR pipeline, atmospheric effects)
- **kiran** — game engine (lighting, lens flares, volumetric effects)
- **tara** — stellar spectra and astrophysical light simulation
- **bijli** — EM wave coupling with optical frequencies

## Architecture

- 608 tests, 162 benchmarks (criterion)
- 9 modules: ray, wave, spectral, lens, pbr, atmosphere, error, ai, logging
- 11,840 lines. Built on hisab for all math primitives

## Roadmap

Stable at 1.1.0. Future: GPU-accelerated ray tracing paths, fluorescence modeling, integration with soorat's shader pipeline.
