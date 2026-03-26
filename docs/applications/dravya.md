# Dravya

> **Dravya** (Sanskrit: substance/matter) — Material science engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/dravya` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/dravya.toml` |
| crates.io | `dravya` |

---

## What It Does

- Stress and strain analysis: tensors, principal stresses, Mohr's circle
- Elasticity: linear and nonlinear constitutive models (Hooke, Neo-Hookean, Ogden)
- Fatigue and fracture: S-N curves, crack propagation, stress intensity factors
- Composite materials: laminate theory, ply stacking, failure criteria (Tsai-Wu, Hashin)
- Material property databases for metals, polymers, ceramics, and composites

## Consumers

- **impetus** — material properties for physics simulation
- **kiran** — structural simulation, destructible environments
- **joshua** — vehicle and structure damage modeling
- Engineering and educational applications

## Architecture

- Built on hisab for tensor math and numerical methods
- Constitutive model trait for pluggable material behaviors
- Dependencies: hisab, serde

## Roadmap

Stable at 1.0.0. Future: finite element method integration, thermal-mechanical coupling (with ushma), additive manufacturing material models.
