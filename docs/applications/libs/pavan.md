# Pavan

> **Pavan** (Sanskrit: wind) — Aerodynamics engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/pavan` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/pavan.toml` |
| crates.io | `pavan` |

---

## What It Does

- Atmosphere models: ISA, US Standard Atmosphere, altitude-dependent properties
- Airfoil analysis: NACA generation, lift/drag coefficients, pressure distributions
- Panel methods and Vortex Lattice Method (VLM) for 3D aerodynamic analysis
- Compressible flow: normal/oblique shocks, Prandtl-Meyer expansion, nozzle design
- Stability derivatives and propulsion models (thrust, specific impulse)

## Consumers

- **kiran** — game engine (flight simulation, wind effects)
- **joshua** — vehicle dynamics, aircraft and projectile simulation
- **badal** — atmospheric wind modeling
- Engineering and educational applications

## Architecture

- Built on pravash for flow field computations
- Atmosphere and airfoil databases included
- Dependencies: pravash, hisab, serde

## Roadmap

Stable at 1.0.0. Future: rotorcraft aerodynamics, store separation analysis, real-time wind tunnel visualization.
