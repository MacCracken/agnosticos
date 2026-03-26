# Khanij

> **Khanij** (Hindi/Sanskrit: mineral) — Geology and mineralogy engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/khanij` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/khanij.toml` |
| crates.io | `khanij` |

---

## What It Does

- Crystal structures: unit cells, Bravais lattices, symmetry operations, Miller indices
- Rock cycles: igneous, sedimentary, metamorphic formation and transformation
- Soil mechanics: classification, compaction, permeability, bearing capacity
- Geochemistry: mineral reactions, dissolution, precipitation (via kimiya integration)
- Tectonics and volcanism: plate motion, magma properties, eruption modeling, weathering

## Consumers

- **kiran** — terrain generation, geological features, cave systems
- **joshua** — environment simulation (geological hazards, resource extraction)
- **kimiya** — geochemical reaction pathways
- Educational and research applications

## Architecture

- Built on hisab for crystallography math, kimiya for chemical properties
- Mineral property database with 200+ entries
- Dependencies: hisab, kimiya, serde

## Roadmap

Stable at 1.0.0. Future: procedural terrain generation algorithms, seismic wave propagation (coupling with goonj), groundwater flow modeling.
