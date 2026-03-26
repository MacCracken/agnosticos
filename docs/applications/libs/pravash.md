# Pravash

> **Pravash** (Sanskrit: flow) — Fluid dynamics simulation

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/pravash` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/pravash.toml` |
| crates.io | `pravash` |

---

## What It Does

- SPH (Smoothed Particle Hydrodynamics) for particle-based fluid simulation
- Euler and Navier-Stokes solvers for grid-based flow
- Shallow water equations for surface flow and wave propagation
- Buoyancy, drag, and lift force calculations
- Vortex dynamics and turbulence modeling

## Consumers

- **pavan** — aerodynamics engine (flow field computations)
- **badal** — weather simulation (atmospheric fluid dynamics)
- **kiran** — game engine (water, smoke, fire effects)
- **ushma** — convective heat transfer coupling

## Architecture

- Built on hisab for linear algebra and spatial structures
- Particle and grid solvers with configurable timestep
- Dependencies: hisab, serde

## Roadmap

Stable at 1.1.0. Future: GPU-accelerated SPH, multiphase flow, coupling with impetus for fluid-structure interaction.
