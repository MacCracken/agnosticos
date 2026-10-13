# Raasta

> **Raasta** (Hindi: path/road) — Navigation and pathfinding library

- **Repository**: [github.com/MacCracken/raasta](https://github.com/MacCracken/raasta)
- **License**: GPL-3.0-only
- **Status**: Stable - See Repo
- **Language**: Rust

See the [libs registry](README.md) for the full catalog and dependency context.

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
