# Ushma

> **Ushma** (Sanskrit: heat) — Thermodynamics simulation

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/ushma` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/ushma.toml` |
| crates.io | `ushma` |

---

## What It Does

- Heat transfer: conduction, convection, radiation (Stefan-Boltzmann, view factors)
- Entropy and free energy calculations (Gibbs, Helmholtz)
- Equations of state: ideal gas, van der Waals, Peng-Robinson
- Thermal properties: specific heat, thermal conductivity, phase transitions
- Temperature field evolution and steady-state solvers

## Consumers

- **kimiya** — reaction thermochemistry (enthalpy, equilibrium constants)
- **kiran** — thermal effects in game environments
- **badal** — atmospheric thermodynamics
- Engineering simulation applications

## Architecture

- Built on hisab for numerical methods
- Pure Rust, deterministic computation
- Dependencies: hisab, serde

## Roadmap

Stable at 1.1.0. Future: multi-phase flow thermodynamics, coupling with pravash for convective heat transfer, HVAC simulation profiles.
