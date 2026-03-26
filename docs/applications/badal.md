# Badal

> **Badal** (Hindi: cloud) — Weather and atmospheric modeling

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/badal` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/badal.toml` |
| crates.io | `badal` |

---

## What It Does

- Weather simulation: temperature, pressure, humidity, precipitation modeling
- Atmospheric dynamics: convection, fronts, cyclone formation, jet streams
- Cloud physics: condensation, droplet growth, ice nucleation
- Solar radiation: insolation, albedo, greenhouse effect calculations
- Terrain interaction: orographic lift, rain shadow, sea breeze circulation

## Consumers

- **kiran** — game engine (dynamic weather effects, sky rendering)
- **joshua** — environment simulation (mission-affecting weather)
- **pavan** — atmospheric conditions for aerodynamic calculations
- **pravash** — atmospheric fluid dynamics coupling

## Architecture

- Built on pravash for fluid dynamics, ushma for thermodynamics, hisab for numerics
- Grid-based atmospheric model with configurable resolution
- Dependencies: pravash, ushma, hisab, serde

## Roadmap

Stable at 1.0.0. Future: real-world weather data ingestion, climate modeling, integration with prakash for atmospheric optical effects (rainbows, halos).
