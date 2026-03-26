# Impetus — Physics Engine

> **Impetus** (Latin: driving force — the medieval theory of why objects keep moving) — shared physics engine for AGNOS

| Field | Value |
|-------|-------|
| Status | Published (1.1.0) |
| Priority | 4 — shared physics for kiran, joshua, simulation, desktop effects |
| Crate | `impetus` (crates.io, available) |
| Repository | `MacCracken/impetus` |
| Runtime | library crate |
| Domain | 2D/3D physics simulation |

---

## Why First-Party

Every physics-enabled application in AGNOS — kiran (game engine), joshua (simulation), aethersafha (desktop effects) — needs the same primitives: rigid bodies, collision detection, constraints, raycasting. Wrapping rapier directly in each consumer duplicates configuration, tuning, and integration work.

Impetus is the thin, opinionated layer over rapier that provides:
- Consistent API across 2D and 3D
- AGNOS-specific defaults (deterministic stepping, fixed timestep)
- Integration points for ECS (kiran), scene graph (aethersafta), and simulation (joshua-sim)
- Serializable world state (TOML/JSON for scene files, libro for replay)
- Headless mode (no rendering dependency — pure simulation)

Impetus does NOT replace rapier. It wraps it the same way ranga wraps image primitives and dhvani wraps audio primitives — a shared, tested, versioned interface that all AGNOS consumers depend on instead of each rolling their own rapier integration.

## Math Layer

Impetus depends on **glam** for vector/matrix/quaternion types (same as rapier and wgpu). For higher math needs (calculus, numerical solvers, spatial geometry algorithms), impetus will depend on **hisab** (planned) once available. Basic math and unit conversions come from **abaco**.

## Design Principles

1. **Wrap, don't rewrite** — rapier is excellent. Impetus provides the AGNOS integration layer: serialization, determinism, ECS compatibility, scene loading.
2. **2D and 3D unified** — single API with `World2D` and `World3D` that share the same `Body`, `Collider`, `Joint` vocabulary. Feature-gated so 2D-only apps don't pull 3D deps.
3. **Deterministic by default** — fixed timestep, deterministic solver. Essential for simulation replay (joshua-sim) and multiplayer sync (majra).
4. **Serializable state** — world state serializes to TOML/JSON for scene files and to bytes for network sync and libro audit.
5. **Zero rendering dependency** — impetus is pure simulation. Rendering is the consumer's job (aethersafta, joshua-render, or headless).

## Architecture

### Where Impetus Sits

```
Kiran (game engine)
  ├── aethersafta   — rendering, scene graph
  ├── impetus       — physics (THIS CRATE)
  ├── ranga         — textures, image processing
  ├── dhvani        — spatial audio
  └── ...

Joshua (game manager + simulation)
  ├── kiran         — game engine (ECS, loop, rendering, audio, input)
  ├── impetus       — physics (THIS CRATE)
  ├── agnosai       — NPC agent orchestration
  ├── majra         — multiplayer
  └── ...

aethersafha (desktop compositor)
  └── impetus       — window physics, spring animations, particle effects

Simulation workloads
  └── impetus       — headless physics for agent training environments
```

### Crate Structure

Single flat crate (like tarang, ranga, dhvani):

```
impetus/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public API, World2D/World3D constructors
│   ├── body.rs         # RigidBody — static, dynamic, kinematic
│   ├── collider.rs     # Collider shapes — box, sphere, capsule, mesh, heightfield
│   ├── joint.rs        # Joints/constraints — fixed, revolute, prismatic, spring
│   ├── world.rs        # PhysicsWorld — step, query, raycast
│   ├── query.rs        # Spatial queries — raycast, shapecast, overlap, closest point
│   ├── material.rs     # PhysicsMaterial — friction, restitution, density
│   ├── force.rs        # Forces, impulses, gravity
│   ├── event.rs        # Collision events, contact data, triggers
│   ├── serialize.rs    # World state → TOML/JSON/bytes
│   ├── config.rs       # WorldConfig — timestep, gravity, solver iterations
│   └── error.rs        # ImpetusError
├── benches/
│   └── simulation.rs   # Criterion: step latency, collision throughput
└── tests/
```

### Key Types

```rust
/// Physics world configuration.
pub struct WorldConfig {
    /// Fixed timestep in seconds (default: 1/60).
    pub timestep: f64,
    /// Gravity vector (default: [0, -9.81] for 2D, [0, -9.81, 0] for 3D).
    pub gravity: Vec<f64>,
    /// Solver iterations (default: 4).
    pub solver_iterations: u32,
    /// Enable deterministic mode (default: true).
    pub deterministic: bool,
}

/// A physics world (2D or 3D behind feature flags).
pub struct PhysicsWorld {
    // rapier pipeline wrapped
}

impl PhysicsWorld {
    /// Create a new world with config.
    pub fn new(config: WorldConfig) -> Self;

    /// Step the simulation by one fixed timestep.
    pub fn step(&mut self);

    /// Add a rigid body, returns handle.
    pub fn add_body(&mut self, desc: BodyDesc) -> BodyHandle;

    /// Add a collider attached to a body.
    pub fn add_collider(&mut self, body: BodyHandle, desc: ColliderDesc) -> ColliderHandle;

    /// Cast a ray into the world.
    pub fn raycast(&self, origin: &[f64], direction: &[f64], max_dist: f64) -> Option<RayHit>;

    /// Get collision events from last step.
    pub fn collision_events(&self) -> &[CollisionEvent];

    /// Serialize world state (for replay/network sync).
    pub fn serialize(&self) -> Vec<u8>;

    /// Restore world state.
    pub fn deserialize(&mut self, data: &[u8]) -> Result<(), ImpetusError>;
}

/// Rigid body descriptor.
pub struct BodyDesc {
    pub body_type: BodyType,
    pub position: Vec<f64>,
    pub rotation: f64,        // radians (2D) or quaternion (3D)
    pub linear_damping: f64,
    pub angular_damping: f64,
}

pub enum BodyType {
    Static,
    Dynamic,
    Kinematic,
}

/// Collider shape descriptor.
pub enum ColliderShape {
    Box { half_extents: Vec<f64> },
    Sphere { radius: f64 },
    Capsule { half_height: f64, radius: f64 },
    ConvexMesh { vertices: Vec<Vec<f64>> },
    TriMesh { vertices: Vec<Vec<f64>>, indices: Vec<[u32; 3]> },
    Heightfield { heights: Vec<Vec<f64>>, scale: Vec<f64> },
}
```

### Scene Integration (TOML)

Physics properties in Joshua scene files:

```toml
[[entity]]
name = "crate"
position = [3.0, 5.0, 0.0]
model = "models/crate.glb"

[entity.physics]
body_type = "dynamic"
mass = 10.0

[entity.physics.collider]
shape = "box"
half_extents = [0.5, 0.5, 0.5]

[entity.physics.material]
friction = 0.6
restitution = 0.3

[[entity]]
name = "floor"
position = [0.0, 0.0, 0.0]

[entity.physics]
body_type = "static"

[entity.physics.collider]
shape = "box"
half_extents = [50.0, 0.1, 50.0]
```

## Feature Flags

```toml
[features]
default = ["2d"]
2d = ["dep:rapier2d"]
3d = ["dep:rapier3d"]
serialize = ["dep:bincode"]
full = ["2d", "3d", "serialize"]
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `rapier2d` | 2D physics backend (feature-gated) |
| `rapier3d` | 3D physics backend (feature-gated) |
| `serde` + `serde_json` + `toml` | Serialization |
| `bincode` | Binary serialization for network/replay (feature-gated) |
| `glam` | Math types (vectors, quaternions) — same as wgpu/rapier use |
| `thiserror` | Error types |
| `tracing` | Structured logging |

## Roadmap

### Phase 1 — 2D Core
- [ ] `PhysicsWorld` wrapping rapier2d pipeline
- [ ] `BodyDesc`, `ColliderDesc`, `BodyType` types
- [ ] Fixed timestep stepping with deterministic mode
- [ ] Collider shapes: box, sphere, capsule, convex
- [ ] Collision events and triggers
- [ ] Raycasting and shape queries
- [ ] Materials (friction, restitution)
- [ ] Forces, impulses, gravity
- [ ] World state serialization (JSON + binary)
- [ ] Criterion benchmarks
- [ ] Tests: 60+

### Phase 2 — 3D + Joints
- [ ] `World3D` wrapping rapier3d (feature-gated)
- [ ] 3D collider shapes: box, sphere, capsule, trimesh, heightfield
- [ ] Joints: fixed, revolute, prismatic, spring, ball
- [ ] Character controller (kinematic body with ground detection)
- [ ] Continuous collision detection (CCD) for fast objects

### Phase 3 — Integration
- [ ] TOML scene loading (physics properties on entities)
- [ ] ECS integration helpers (for joshua-core)
- [ ] Network state sync format (for majra multiplayer)
- [ ] Libro integration (physics state snapshots for simulation replay)
- [ ] Debug rendering data (wireframe collider outlines for aethersafta)

### Phase 4 — Advanced
- [ ] Soft bodies / cloth simulation
- [ ] Fluid simulation (SPH particles)
- [ ] Vehicle physics (wheels, suspension)
- [ ] Destructible objects (fracture on impact)
- [ ] LOD physics (simplified colliders at distance)

## Reference Code

| Source | What to Reference | Path | Maturity |
|--------|------------------|------|----------|
| **Rapier** | Physics engine internals, API patterns | External (rapier.rs) | High — standard Rust physics |
| **Bevy Rapier** | ECS integration patterns for rapier | External (bevy_rapier) | High — proven integration |
| **Aethersafta** | Scene graph, rendering pipeline, compositing | `/home/macro/Repos/aethersafta/src/` | High — published (0.21.3) |
| **Ranga** | GPU compute patterns (for future GPU physics) | `/home/macro/Repos/ranga/src/` | High — published (0.21.4) |
| **Dhvani** | Spatial positioning patterns (audio uses same 3D math) | `/home/macro/Repos/dhvani/src/` | High — published (0.21.4) |

---

*Last Updated: 2026-03-22*
