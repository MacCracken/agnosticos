# Tanmatra

> **Tanmatra** (Sanskrit: subtle element) — Atomic and subatomic physics

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/tanmatra` |
| Runtime | library crate (Rust) |

---

## What It Does

- Standard Model particles: quarks, leptons, bosons, fundamental forces
- Nuclear structure: Bethe-Weizsacker binding energy, nuclear radii
- Radioactive decay: half-lives, decay chains, decay modes
- Nuclear reactions: Q-values, Coulomb barriers
- Atomic physics: electron configurations, spectral lines (Balmer, Lyman, etc.), ionization energies
- Data sourced from CODATA 2022, PDG 2024, NNDC/NUBASE, NIST ASD
- Optional optics integration via prakash (spectral line wavelengths)
- `no_std` + `alloc` compatible
- Zero `unsafe` code

## Consumers

- **prakash** — optics library (spectral line coupling)
- **kimiya** — chemistry library (atomic structure, isotope data)
- **kiran** — game engine (physics simulation data)
