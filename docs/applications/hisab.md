# Hisab

> **Hisab** (Arabic: calculation/mathematics) — Higher mathematics library

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/hisab` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/hisab.toml` |
| crates.io | `hisab` |

---

## What It Does

- Linear algebra: vectors, matrices, quaternions, decompositions (LU, QR, SVD, Cholesky)
- Geometry: 2D/3D primitives, transforms, intersections, convex hull
- Calculus: numerical integration, differentiation, ODE solvers
- Numerical methods: root finding, interpolation, optimization, FFT
- Spatial structures: BVH, k-d tree, octree, GJK/EPA collision queries

## Consumers

- **prakash** — optics and light simulation (spectral math, lens geometry)
- **impetus** — physics engine (uses hisab types natively instead of glam)
- **kiran** — game engine (math primitives throughout)
- **joshua** — game manager and simulation
- All AGNOS science and engineering crates

## Architecture

- 360 tests, 82 benchmarks (criterion)
- Pure Rust, no_std compatible core
- Types designed as drop-in replacements for glam with richer functionality

## Roadmap

Stable at 1.1.0. Foundation crate for the entire AGNOS science stack. Future: sparse matrix support, GPU-accelerated paths via compute shaders.
