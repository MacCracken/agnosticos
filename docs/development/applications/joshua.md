# Joshua — Game Manager & Simulation Core

> **Joshua** (WarGames: "Shall we play a game?") — AI-native game manager and simulation runtime for AGNOS

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 4 — simulation infrastructure + AI game management |
| Crate | `joshua` (crates.io, available) |
| Repository | `MacCracken/joshua` |
| Runtime | library crate + binary (editor/runner) |
| Domain | Game management / simulation / AI agents in virtual environments |
| Engine | Builds on **kiran** (game engine) + **impetus** (physics) |

---

## Why First-Party

No game engine treats AI agents as first-class citizens. Unity and Unreal bolt on ML as a plugin. Godot has no LLM integration. Bevy is Rust-native but has no agent runtime, no LLM-powered NPCs, no simulation mode for training AI in virtual environments.

AGNOS already has the pieces: kiran for the game engine core (ECS, game loop, rendering, audio, input), impetus for physics, hoosh for LLM inference, daimon for agent orchestration, murti for model serving. Joshua sits on top as the **game manager and simulation runtime** where:

- NPCs are daimon agents with LLM brains (via hoosh)
- Game worlds are simulation environments for training/testing AI behavior
- Engine core (ECS, rendering, audio, input) is provided by **kiran**
- Physics is provided by **impetus**
- Multiplayer uses majra pub/sub for networking
- Game logic is sandboxed via kavach

Joshua isn't a game engine — that's kiran. Joshua is the **game manager and simulation runtime**. Train AI agents in virtual environments, test autonomous behavior before deploying to the real world, and ship games as a side effect.

## Design Principles

1. **Agents are players** — NPCs aren't scripted state machines. They're daimon agents with goals, memory, and LLM reasoning via hoosh. They observe the world, think, and act.
2. **Simulation first, graphics second** — Joshua runs headless for training/testing. Rendering is optional. A simulation can run 1000x faster than real-time without a window.
3. **ECS architecture** — Entity-Component-System for performance and composability. Aligns with Rust's ownership model.
4. **Deterministic replay** — every simulation frame is reproducible from a seed + input log. Essential for debugging AI behavior and training reproducibility.
5. **Hot reload** — game logic (scripts, agent behavior, shaders) reloads without restarting. Development iteration in milliseconds.

## Architecture

### The Stack

Joshua is the game manager and simulation layer on top of kiran (game engine) and impetus (physics). It owns simulation, AI NPCs, the editor, and multiplayer — not the engine core.

```
Joshua (game manager + simulation)
  ├── joshua-sim      — Headless simulation mode, batch training, metrics
  ├── joshua-npc      — AI NPC system (perception, memory, reasoning, action)
  ├── joshua-editor   — Visual editor (egui)
  │
  │   Engine layer (kiran owns these):
  │
  ├── kiran           — ECS, game loop, scene format, rendering, audio, input
  │     ├── aethersafta — rendering, scene graph, compositing (wgpu)
  │     ├── dhvani      — spatial audio, mixing, synthesis
  │     └── ranga       — texture/image processing, GPU compute
  │
  │   Shared crates:
  │
  ├── impetus         — 2D/3D physics (rapier wrapper)
  ├── agnosai         — NPC agent orchestration (crews, tasks, tools)
  ├── tarang          — video recording, asset transcoding
  ├── majra           — multiplayer networking, event pub/sub
  ├── kavach          — WASM script sandboxing
  ├── libro           — simulation audit trail
  └── t-ron           — NPC tool call security
```

### AGNOS Integration

```
Joshua (game manager + simulation)
  │
  ├── kiran      — game engine (ECS, game loop, scenes, rendering, audio, input)
  │     ├── aethersafta — rendering pipeline
  │     │     └── ranga — texture processing, GPU shaders
  │     └── dhvani     — spatial audio, music, SFX
  │
  ├── agnosai    — NPCs are crews/agents with tasks and tools
  │     └── hoosh (socket) — LLM reasoning for NPC decisions
  │           └── murti — local model for fast NPC inference
  │
  ├── impetus    — physics simulation
  ├── tarang     — video recording, cutscene export
  ├── majra      — multiplayer networking
  ├── kavach     — sandboxed game scripts (WASM)
  ├── libro      — simulation audit trail (replay reproducibility)
  ├── t-ron      — security on NPC tool calls
  └── ai-hwaccel — GPU detection for rendering + inference
```

### What Makes Joshua Different

| Feature | Traditional Engines | Joshua |
|---------|-------------------|--------|
| NPC AI | Scripted behavior trees, FSMs | **agnosai** crews — LLM agents that reason, remember, and adapt |
| Rendering | Custom renderer | **aethersafta** — scene graph, compositing, wgpu (Vulkan/Metal/DX12/WebGPU) |
| Physics | Built-in or plugin | **impetus** — shared crate, rapier-backed, deterministic, serializable |
| Simulation | Real-time only | **Headless mode** — 1000x speed for AI training |
| Multiplayer | Custom netcode | **majra** pub/sub — battle-tested distributed messaging |
| Scripting | Lua/GDScript/C# | **WASM** sandboxed via kavach — any language compiles to WASM |
| Audio | FMOD/Wwise | **dhvani** — pure Rust, spatial audio, synthesis |
| Media | External tools | **tarang** — record gameplay, transcode, stream |
| Image/Texture | Engine-specific | **ranga** — GPU compute, color spaces, filters, blend modes |
| Security | None | **t-ron** — NPC tool calls are audited and rate-limited |
| Replay | Partial/none | **Deterministic** — every frame reproducible from seed |
| GPU | Engine-specific | **ai-hwaccel** — shared GPU detection with inference |

### AI NPC Architecture

```
NPC Agent (daimon-registered)
  ├── Perception   — what the NPC sees/hears (ECS queries on nearby entities)
  ├── Memory       — short-term (last N observations) + long-term (vector store via daimon RAG)
  ├── Reasoning    — LLM call via hoosh ("given what I see and remember, what should I do?")
  ├── Action       — execute chosen action in the game world (move, speak, interact)
  └── Personality  — system prompt defining character traits, goals, knowledge
```

**Inference modes:**
- **Local fast** — small model via murti for real-time decisions (<50ms)
- **Cloud deep** — large model via hoosh cloud fallback for complex reasoning
- **Cached** — common situations have cached responses (hoosh cache layer)
- **Batch** — headless simulation batches NPC decisions for throughput

### Simulation Mode

Joshua's headless simulation mode is the killer feature for AI development:

```rust
use joshua::sim::{Simulation, SimConfig};

// Run 1000 episodes of agents learning to navigate a maze
let config = SimConfig {
    headless: true,
    speed: SimSpeed::Unlimited,  // As fast as CPU allows
    episodes: 1000,
    seed: 42,                     // Deterministic
    record_metrics: true,
};

let mut sim = Simulation::new("maze_world.scene", config).await?;
sim.spawn_agent("navigator", agent_config).await?;

let results = sim.run().await?;
// results.episodes[0].steps, .reward, .success, .agent_decisions
```

Use cases:
- **RL training** — agents learn in simulated environments
- **Behavior testing** — verify AI agent behavior before deployment
- **Load testing** — simulate 1000 agents interacting simultaneously
- **Game balancing** — AI plays the game 10,000 times to find balance issues
- **Procedural generation testing** — validate generated content is playable

## Crate Structure

Joshua is thin — only 3 internal crates. Engine core (ECS, game loop, input) lives in kiran. Everything else is an existing AGNOS shared crate.

```
joshua/
├── Cargo.toml           # Workspace root
├── crates/
│   ├── joshua-sim/      # Headless simulation, batch training, metrics
│   ├── joshua-npc/      # AI NPC system (perception, memory, reasoning, action)
│   └── joshua-editor/   # Visual editor (egui)
├── src/
│   └── main.rs          # CLI: joshua run/edit/sim/export
└── examples/
    ├── hello_world.rs   # Spinning cube (uses kiran)
    ├── npc_chat.rs      # LLM-powered NPC conversation
    ├── maze_sim.rs      # Headless agent training
    └── multiplayer.rs   # Networked game via majra
```

## Dependencies

### AGNOS Shared Crates (the engine's real power)

| Crate | Purpose | What Joshua would build without it |
|-------|---------|-----------------------------------|
| `agnosai` | NPC agent orchestration (crews, tasks, tools) | Custom NPC system |
| `aethersafta` | Rendering, scene graph, compositing (wgpu) | Custom renderer |
| `impetus` | 2D/3D physics (rapier wrapper, deterministic) | Custom physics integration |
| `dhvani` | Spatial audio, mixing, synthesis | Custom audio engine |
| `ranga` | Texture processing, GPU shaders, image effects | Custom image pipeline |
| `tarang` | Video recording, asset transcoding | Custom media handling |
| `majra` | Multiplayer networking, event pub/sub | Custom netcode |
| `kavach` | WASM script sandboxing | Custom sandbox |
| `libro` | Simulation audit trail | Custom replay logging |
| `t-ron` | NPC tool call security | No security layer |
| `ai-hwaccel` | GPU detection | Custom hardware detection |

### Engine Dependency

| Crate | Purpose |
|-------|---------|
| `kiran` | Game engine core — ECS, game loop, scene format, rendering, audio, input |

### External Crates

| Crate | Purpose |
|-------|---------|
| `egui` | Editor UI |
| `glam` | Math (vectors, matrices, quaternions) |

NPC inference goes through hoosh socket — not a direct dependency. NPCs are agnosai agents registered with daimon. Window management and rendering are kiran's responsibility.

## Scene Format

TOML-based (consistent with AGNOS):

```toml
[scene]
name = "tavern"
ambient_light = [0.2, 0.2, 0.3]

[[entity]]
name = "bartender"
type = "npc"
position = [5.0, 0.0, 3.0]

[entity.ai]
model = "mistral:7b-q4"
personality = """
You are Greta, a gruff but kind tavern keeper.
You've run this tavern for 30 years. You know everyone's secrets.
You speak in short sentences and never smile, but always refill drinks.
"""
memory_enabled = true
tools = ["speak", "give_item", "refuse", "emote"]

[[entity]]
name = "table"
type = "static"
position = [3.0, 0.0, 2.0]
model = "models/tavern/table.glb"
physics = "static"

[[entity]]
name = "candle"
type = "light"
position = [3.0, 1.2, 2.0]
color = [1.0, 0.8, 0.5]
intensity = 2.0
```

## Roadmap

### Phase 1 — Simulation Core (joshua-sim + joshua-npc)
- [ ] kiran integration (ECS, game loop, scenes, rendering, audio, input)
- [ ] impetus physics integration (2D, deterministic stepping)
- [ ] Deterministic replay (seed + input log)
- [ ] CLI: `joshua run scene.toml` (delegates to kiran engine)
- [ ] Tests: 80+

### Phase 2 — AI NPCs (agnosai integration)
- [ ] NPC type backed by agnosai agents (crew = party, agent = NPC)
- [ ] Perception system (ECS queries for nearby entities → agent context)
- [ ] LLM reasoning via hoosh socket (agnosai tool calls)
- [ ] NPC memory (short-term ring buffer + daimon RAG for long-term)
- [ ] Behavior trees (fallback when LLM is slow)
- [ ] Pathfinding (A* on nav mesh via raasta — grid, navmesh, flow fields, steering)
- [ ] NPC actions as agnosai tools (move, speak, interact, emote)
- [ ] t-ron integration (audit NPC tool calls)

### Phase 3 — Simulation Mode (joshua-sim)
- [ ] Headless runner (`joshua sim`) — no aethersafta, impetus only
- [ ] Unlimited speed mode (no render, no sleep)
- [ ] Batch episode execution
- [ ] Metrics collection (reward, steps, success rate)
- [ ] libro audit trail per simulation
- [ ] Agent behavior recording and replay
- [ ] Environment randomization (procedural variation per episode)

### Phase 4 — Editor & Polish (joshua-editor)
- [ ] Visual editor (egui): scene editing, entity inspector, real-time preview
- [ ] impetus 3D physics integration
- [ ] WASM scripting via kavach
- [ ] Multiplayer via majra
- [ ] Video recording via tarang
- [ ] Asset pipeline (glTF via aethersafta, audio via tarang/dhvani)
- [ ] ranga texture/image processing pipeline
- [ ] MCP tools: `joshua_run`, `joshua_sim`, `joshua_scene`, `joshua_npc`
- [ ] Agnoshi intents: "joshua run tavern", "joshua sim maze 1000 episodes"
- [ ] Marketplace recipe

### Phase 5 — Advanced
- [ ] 3D physics (rapier3d)
- [ ] Particle systems
- [ ] Skeletal animation
- [ ] Terrain generation
- [ ] WebGPU export (run in browser)
- [ ] VR/XR support
- [ ] Procedural world generation via LLM

### Phase 6 — Quantum Simulation (post agnos-kernel)
- [ ] Quantum execution backend for `joshua-sim` (alongside classical)
- [ ] Quantum state representation in ECS (superposition as component state)
- [ ] Parallel path exploration — agents explore all branches simultaneously
- [ ] Quantum-classical hybrid mode — quantum for search/optimization, classical for rendering/IO
- [ ] Quantum advantage workloads: combinatorial optimization (pathfinding, scheduling), sampling (procedural generation), search (game tree exploration)
- [ ] Deterministic replay from quantum measurement seeds
- [ ] Integration with AGNOS quantum kernel abstraction layer (agnosys quantum backend)
- [ ] Quantum-aware murti — route inference to quantum accelerator for supported operations
- [ ] Benchmark suite: classical vs quantum simulation speedup per workload type

## Reference Code

| Source | What to Reference | Path | Maturity |
|--------|------------------|------|----------|
| **Bevy** | ECS architecture, plugin system, game loop patterns | External (bevy.rs) | High — proven Rust game engine |
| **AgnosAI** | Agent orchestration — crews, tasks, tools. NPC backbone | `/home/macro/Repos/agnosai/` | High — published (0.20.3), 4574x faster than CrewAI |
| **Aethersafta** | Rendering, scene graph, compositing, wgpu pipeline | `/home/macro/Repos/aethersafta/src/` | High — published (0.21.3) |
| **Impetus** | Physics wrapper over rapier, deterministic stepping | Planned — see [impetus.md](impetus.md) | Planned |
| **Dhvani** | Spatial audio, DSP, mixing, synthesis | `/home/macro/Repos/dhvani/src/` | High — published (0.21.4) |
| **Ranga** | Texture/image processing, GPU compute | `/home/macro/Repos/ranga/src/` | High — published (0.21.4) |
| **Tarang** | Video recording, media pipeline | `/home/macro/Repos/tarang/src/` | High — published (0.21.3) |
| **Majra** | Multiplayer networking, pub/sub, heartbeat | `/home/macro/Repos/majra/src/` | High — published (0.21.3) |
| **Kavach** | WASM sandboxing for game scripts | `/home/macro/Repos/kavach/src/backend/wasm/` | High — published (0.21.3) |
| **T-Ron** | NPC tool call security | `/home/macro/Repos/t-ron/src/` | Scaffolded (0.1.0) |
| **Libro** | Simulation audit trail, replay reproducibility | `/home/macro/Repos/libro/src/` | High — published (0.21.3), 17 tests |
| **Nazar** | egui desktop app patterns | `/home/macro/Repos/nazar/` | High — reference for editor UI |
| **Daimon** | Agent lifecycle, RAG, vector store | `userland/agent-runtime/src/` | High — 3897+ tests |
| **SecureYeoman** | Simulation/game management patterns | `/home/macro/Repos/secureyeoman/packages/core/src/simulation/` | Medium — reference patterns |

## Why This Matters

The simulation use case is as important as the game use case. Every AGNOS agent — from Agnostic crews to SecureYeoman workflows — could benefit from simulated testing before real deployment. Joshua provides the virtual world where agents prove themselves safe and effective before touching real systems.

WarGames taught us that the best move is sometimes not to play. Joshua teaches agents the best move by letting them play a million times first.

### Quantum Horizon

Joshua is designed with abstraction layers that survive a hardware paradigm shift. The ECS doesn't care if entity state is classical bits or qubits. The scene format is declarative — it describes *what*, not *how*. The simulation runner is a trait, not a concrete implementation.

When AGNOS gains a quantum kernel (post agnos-kernel, Phase 20+), Joshua's simulation mode becomes a quantum simulation mode. Same scenes, same agents, same replay format — different physics underneath. Classical Joshua explores paths sequentially. Quantum Joshua explores all paths simultaneously.

This is the long game: build the abstraction right on classical hardware today so the quantum transition is a backend swap, not a rewrite.

---

*Last Updated: 2026-03-22 (kiran separation)*
