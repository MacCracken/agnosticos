# Bijli

> **Bijli** (Hindi: lightning/electricity) — Electromagnetism simulation

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/bijli` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/bijli.toml` |
| crates.io | `bijli` |

---

## What It Does

- Electric fields: Coulomb's law, Gauss's law, potential, capacitance
- Magnetic fields: Biot-Savart, Ampere's law, inductance, Lorentz force
- Maxwell's equations: full FDTD solver for electromagnetic wave propagation
- Charge dynamics: particle-in-cell, drift-diffusion, circuit elements
- EM wave simulation: polarization, reflection, transmission, waveguides

## Consumers

- **prakash** — wave optics EM coupling at optical frequencies
- **kiran** — game engine (lightning effects, electromagnetic puzzles)
- **goonj** — acoustic-electromagnetic transducer modeling
- Educational and engineering simulation applications

## Architecture

- Built on hisab for vector calculus and spatial structures
- FDTD grid solver with PML boundary conditions
- Dependencies: hisab, serde

## Roadmap

Stable at 1.0.0. Future: GPU-accelerated FDTD, antenna design tools, coupling with pravash for magnetohydrodynamics.
