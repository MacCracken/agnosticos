# Kimiya

> **Kimiya** (Arabic: alchemy/chemistry) — Chemistry engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/kimiya` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/kimiya.toml` |
| crates.io | `kimiya` |

---

## What It Does

- Periodic table: full element data, isotopes, electron configurations
- Molecular modeling: formula parsing, molecular weight, bond types
- Reaction balancing, stoichiometry, and yield calculation
- Chemical kinetics: rate laws, Arrhenius equation, reaction mechanisms
- Thermochemistry: enthalpy, entropy, Gibbs free energy (via ushma integration)

## Consumers

- **khanij** — geochemistry (mineral reactions, weathering chemistry)
- **tara** — stellar nucleosynthesis and astrochemistry
- Educational and research applications on AGNOS

## Architecture

- Built on hisab for numerical methods, ushma for thermodynamic data
- Pure Rust, comprehensive element database
- Dependencies: hisab, ushma, serde

## Roadmap

Stable at 1.0.0. Future: molecular dynamics integration, electrochemistry module, reaction pathway search via AI (hoosh).
