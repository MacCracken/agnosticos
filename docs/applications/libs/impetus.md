# Impetus

> **Impetus** (Latin: driving force) — Physics engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/impetus` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/impetus.toml` |
| crates.io | `impetus` |

---

## What It Does

- 2D and 3D rigid body dynamics with deterministic stepping
- Collision detection and response (broad-phase and narrow-phase)
- Constraints: joints, springs, motors, contact manifolds
- Spatial queries: ray casting, shape casting, point queries
- Rapier wrapper using hisab types natively (not glam re-exports)

## Consumers

- **kiran** — game engine (primary physics backend)
- **joshua** — game manager and simulation (vehicle dynamics, environmental physics)
- **dravya** — material properties fed into structural simulation
- Any AGNOS application needing physics simulation

## Architecture

- Built on Rapier with a hisab-native type layer
- Deterministic stepping for reproducible simulations and netcode
- Dependencies: rapier2d/rapier3d, hisab, serde

## Roadmap

Stable at 1.1.0. Future: soft body simulation, cloth physics, fluid-structure interaction (coupling with pravash).
