# Raasta

> **Raasta** (Hindi: path/road) — Navigation and pathfinding library

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.26.3` |
| Repository | `MacCracken/raasta` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/raasta.toml` |
| crates.io | N/A (not yet published) |

---

## What It Does

- A* pathfinding with customizable heuristics and cost functions
- HPA* (Hierarchical Pathfinding A*) for large-scale maps with precomputed clusters
- Navigation mesh generation and query for 3D environments
- Spatial navigation with obstacle avoidance and steering behaviors
- Grid, graph, and navmesh representations with unified query API

## Consumers

- **kiran** — Game engine (entity navigation, AI movement)
- **joshua** — Game manager (NPC navigation and patrol routes)

## Architecture

- Generic over graph type via trait-based abstraction
- Built on hisab spatial structures (BVH, k-d tree) for acceleration
- Dependencies: serde, hisab

## Roadmap

Pre-release — available but not yet published on crates.io. Future: dynamic obstacle avoidance (RVO2), flow field pathfinding, crowd simulation.
