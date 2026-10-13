# Kiran

> **Kiran** (Sanskrit: ray of light) — AI-native game engine

- **Repository**: [github.com/MacCracken/kiran](https://github.com/MacCracken/kiran)
- **License**: GPL-3.0-only
- **Status**: Stable - See Repo
- **Language**: Rust

See the [libs registry](README.md) for the full catalog and dependency context.

---

## What It Does

- ECS (Entity-Component-System) with Vec arena storage, archetype queries, and change tracking
- System scheduling with stages, job parallelism, and game clock
- Scene hierarchy and state management
- Input handling and event bus
- Asset management with hot reload
- Animation system
- Gizmos for debug visualization
- Profiler for frame timing
- Scripting via kavach sandboxing
- Feature-gated integrations:
  - `ai` — AI behaviors via hoosh
  - `audio` — spatial audio via dhvani
  - `physics` — rigid body physics via impetus (2D and 3D)
  - `rendering` — GPU rendering via soorat
  - `multiplayer` — networking via majra
  - `personality` — NPC personality via bhava
  - `navigation` — pathfinding via raasta
  - `fluids` — fluid simulation via pravash (SPH, shallow water)
  - `acoustics` — acoustic simulation via goonj

## Consumers

- **joshua** — game manager and simulation (builds on kiran for game logic)
- **salai** — game editor (scene editing, preview)
