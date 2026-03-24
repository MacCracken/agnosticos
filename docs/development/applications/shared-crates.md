# Shared Crates — Ecosystem Infrastructure

> **Status**: Active | **Last Updated**: 2026-03-24
>
> Standalone crates extracted from AGNOS that the entire ecosystem depends on.
> Published to crates.io, used by AGNOS, Irfan, AgnosAI, SecureYeoman, and consumer apps.
> See [First-Party Standards](first-party-standards.md) for versioning and publishing conventions.

---

## Registry

| Crate | Version | Description | Consumers |
|-------|---------|-------------|-----------|
| [ai-hwaccel](https://github.com/MacCracken/ai-hwaccel) | 0.21.3 | Universal AI hardware accelerator detection (13 families), quantisation, sharding, training memory estimation | hoosh, daimon, Irfan, AgnosAI, murti, tazama |
| [tarang](https://github.com/MacCracken/tarang) | 0.20.3 | AI-native media framework — 18-33x faster than GStreamer. Audio/video decode, encode, mux, fingerprint, analysis | jalwa, tazama, shruti, aethersafta |
| [aethersafta](https://github.com/MacCracken/aethersafta) | 0.20.3 | Real-time media compositing — scene graph, multi-source capture, HW encoding, streaming output | aethersafha, streaming app, tazama, SY, selah |
| [hoosh](https://github.com/MacCracken/hoosh) | 0.21.3 | AI inference gateway — 15 LLM providers, OpenAI-compatible API, token budgets, caching. Uses murti for local inference | daimon, tarang, aethersafta, agnoshi, AgnosAI, all consumer apps |
| [ranga](https://github.com/MacCracken/ranga) | 0.21.4 | Core image processing — color spaces, blend modes, pixel buffers, filters, GPU compute | rasa, tazama, aethersafta, streaming app |
| [dhvani](https://github.com/MacCracken/dhvani) | 0.20.4 | Core audio engine — buffers, DSP, mixing, resampling, analysis, synthesis, MIDI, clock, PipeWire capture | shruti, jalwa, aethersafta, tarang, hoosh, streaming app |
| [majra](https://github.com/MacCracken/majra) | 0.21.3 | Distributed queue & multiplex engine — pub/sub, priority queues, DAG scheduling, heartbeat FSM, relay, IPC, rate limiting | daimon, AgnosAI, hoosh, sutra, stiva, aethersafta, streaming app |
| [kavach](https://github.com/MacCracken/kavach) | 0.23.3 | Sandbox execution framework — 8 backends (process, gVisor, Firecracker, WASM, OCI, SGX, SEV, SY-AGNOS), strength scoring, policy engine, credential proxy | SY, daimon, stiva, AgnosAI, kiran |
| [libro](https://github.com/MacCracken/libro) | 0.21.3 | Cryptographic audit chain — tamper-proof SHA-256 hash-linked event logging, severity levels, agent tracking | daimon, aegis, stiva, sigil, ark |
| [bote](https://github.com/MacCracken/bote) | 0.21.3 | MCP core service — JSON-RPC 2.0, tool registry, schema validation, dispatch. Eliminates 23 duplicate MCP implementations | all consumer apps with MCP tools |
| [szal](https://github.com/MacCracken/szal) | 0.21.3 | Workflow engine — step/flow execution with branching, retry, rollback, parallel stages | daimon, AgnosAI, sutra |
| **murti** | **0.1.0** | **Core model runtime — registry, store, pull, 15 inference backends, GPU allocation. Extracted from Irfan** | **hoosh, Irfan** |
| **stiva** | **0.1.0** | **OCI container runtime — image management, container lifecycle, overlay FS. Builds on kavach + majra** | **daimon, sutra** |
| **nein** | **0.1.0** | **Programmatic nftables firewall — rule builder, NAT, network policies, container networking** | **stiva, daimon, aegis, sutra** |
| **impetus** | **0.1.0** | **Physics engine — native rigid bodies, collision, constraints, spatial hash broadphase, particles** | **kiran, joshua, aethersafha** |
| [abaco](https://github.com/MacCracken/abaco) | 0.22.4 | Basic math + DSP — expression parser, unit conversion, dB/amplitude, MIDI, panning, filters | abacus, dhvani, hisab |
| **hisab** | **0.22.3** | **Higher math — linear algebra, calculus, geometry, numerical methods, spatial structures (BVH, k-d tree, octree, GJK/EPA)** | **impetus, kiran, joshua, aethersafha** |
| **bhava** | **0.2.0** | **Emotion/personality engine — 11 trait spectrums, PAD mood vectors, archetypes, sentiment, presets** | **SY, joshua, agnosai** |
| **yukti** | **0.22.3** | **Device abstraction — USB, optical, block devices, udev hotplug, mount/eject** | **jalwa, file manager, aethersafha, argonaut** |
| **phylax** | **0.22.3** | **Threat detection — YARA rules, entropy analysis, magic bytes, binary classification** | **daimon, aegis, t-ron** |
| **soorat** | **0.1.0** | **GPU rendering engine — wgpu, sprites, vertices, color, window management. Integrates prakash for optics** | **kiran, salai, joshua** |
| **salai** | **0.1.0** | **Game editor — egui-based visual editor for kiran. Inspector, hierarchy, viewport, gizmos** | **kiran (consumer)** |
| **muharrir** | **0.1.0** | **Shared editor primitives — undo/redo (libro), expression eval (abaco), hierarchy trees, property inspector, hardware detection (ai-hwaccel)** | **salai, rasa, tazama, shruti** |
| **prakash** | **0.23.3** | **Optics/light simulation — ray optics, wave optics, spectral math, lens geometry, PBR math** | **soorat, kiran, ranga** |
| **raasta** | **0.1.0** | **Navigation/pathfinding — grid A*, navmesh, flow fields, path smoothing, steering behaviors** | **kiran, joshua** |
| **kana** | **0.1.0** | **Quantum mechanics — state vectors, Hilbert spaces, operators, entanglement, circuits** | **joshua, kiran** |
| **bijli** | **0.1.0** | **Electromagnetism — fields, Maxwell's equations, charge dynamics, EM waves** | **kiran, joshua, prakash** |
| **ushma** | **0.1.0** | **Thermodynamics — heat transfer, entropy, equations of state, thermal materials** | **kiran, joshua** |
| [pravash](https://github.com/MacCracken/pravash) | 0.2.0 | Fluid dynamics — SPH, Euler/Navier-Stokes, boundary conditions | kiran, joshua |

### Physics & Simulation Crates

Built on hisab for math, each owning a specific domain of physical simulation:

| Crate | Name Origin | Domain | Foundation | Status |
|-------|-------------|--------|------------|--------|
| **[prakash](https://github.com/MacCracken/prakash)** | Sanskrit: प्रकाश (light) | Optics — ray tracing, wave optics, spectral math, PBR | hisab (geo, calc), ranga (rendering) | **Published (0.23.3)** |
| **[kana](https://github.com/MacCracken/kana)** | Sanskrit: कण (particle) | Quantum mechanics — state vectors, Hilbert spaces, operators, entanglement, circuits | hisab (num: complex linear algebra, tensor products) | **Scaffolded (0.1.0)** |
| **[bijli](https://github.com/MacCracken/bijli)** | Hindi: बिजली (electricity) | Electromagnetism — fields, Maxwell's equations, charge dynamics, EM waves | hisab (calc: PDE solvers), impetus (particles) | **Scaffolded (0.1.0)** |
| **[pravash](https://github.com/MacCracken/pravash)** | Sanskrit: प्रवाह (flow) | Fluid dynamics — SPH, Euler/Navier-Stokes | hisab (num: PDE solvers, FFT) | **Scaffolded (0.2.0)** |
| **[ushma](https://github.com/MacCracken/ushma)** | Sanskrit: ऊष्मा (heat) | Thermodynamics — heat transfer, entropy, equations of state, thermal materials | hisab (calc: ODE/PDE), impetus (body contacts) | **Scaffolded (0.1.0)** |

Each is its own flat crate. impetus stays focused on rigid bodies, colliders, constraints, and particles at the classical macro scale. The domain crates share hisab's math but solve fundamentally different equations (PDEs on grids vs ODEs per body).

---

## Status Summary

| Crate | Status | Notes |
|-------|--------|-------|
| **majra** | Released (0.21.3) | Replaced planned "sluice" crate. Pub/sub, priority queues, DAG scheduling, heartbeat FSM, relay, IPC, rate limiting, SQLite persistence. Benchmarked, proptested |
| **kavach** | Released (0.23.3) | 8 sandbox backends, strength scoring (0-100), policy engine, credential proxy, lifecycle management, externalization gate. WASM feature fixed. |
| **nein** | Scaffolded (0.1.0) | nftables rule builder, NAT, network policies, container bridge builders. 24 tests. [README](https://github.com/MacCracken/nein) |
| **stiva** | Scaffolded (0.1.0) | OCI container runtime. Builds on kavach + majra. 17 tests. [Spec](stiva.md) |
| **murti** | Scaffolded (0.1.0) | Core model runtime, extracted from Irfan. 21 tests. [Spec](murti.md) |
| **impetus** | Scaffolded (0.1.0) | Physics engine, rapier wrapper. [Spec](impetus.md) |
| **abaco** | Scaffolded (0.1.0) | Basic math + unit conversion library. 61 tests |
| **t-ron** | Scaffolded (0.1.0) | MCP security monitor, bote middleware. [Spec](t-ron.md) |
| **soorat** | Scaffolded (0.1.0) | GPU rendering engine — wgpu, sprites, vertices, color, window. Integrates prakash. 38 tests |
| **salai** | Scaffolded (0.1.0) | Game editor for kiran — egui, inspector, hierarchy, viewport. 25 tests |
| **raasta** | Scaffolded (0.1.0) | Navigation/pathfinding — grid A*, navmesh, flow fields, steering. 59 tests |
| **kana** | Scaffolded (0.1.0) | Quantum mechanics — state vectors, operators, entanglement, circuits. Builds on hisab |
| **bijli** | Scaffolded (0.1.0) | Electromagnetism — fields, Maxwell, charge dynamics, EM waves. Builds on hisab + impetus |
| **ushma** | Scaffolded (0.1.0) | Thermodynamics — heat transfer, entropy, equations of state, materials. Builds on hisab + impetus |

See [k8s-roadmap.md](../k8s-roadmap.md) for how stiva + nein + majra + kavach compose into a k8s-equivalent orchestration platform.

---

## Ranga — Shared Image Processing Core

| Field | Value |
|-------|-------|
| Status | **Scaffolding** |
| Priority | Infrastructure — enables dedup across rasa, tazama, aethersafta |
| Repository | `MacCracken/ranga` |

**Why**: Rasa, tazama, and aethersafta all implement overlapping image processing: color space conversions (BT.601 in 3 different implementations), alpha blending (Porter-Duff in 2 implementations), pixel buffer types (3 incompatible types), and color correction (histogram analysis duplicated). Extracting a shared crate eliminates ~2000 lines of duplicate code and ensures consistent behavior.

**What gets extracted**:
- Color math: sRGB<>linear, HSL, BT.601/709 YUV<>RGB, ICC profiles (from rasa-core)
- Blend modes: 12 Porter-Duff modes (from rasa-engine)
- Pixel buffers: unified RGBA/RGB/YUV buffer type with format conversion (replaces 3 types)
- CPU filters: brightness, contrast, saturation, levels, curves (from rasa-engine)
- GPU compute: wgpu abstraction for portable Vulkan/Metal shaders (from rasa-gpu)
- SIMD: SSE2/AVX2/NEON alpha blending (from aethersafta)

**Consumers after extraction**:
- **rasa** -> drops rasa-core color math, uses `ranga::color`, `ranga::blend`, `ranga::filter`
- **tazama** -> drops manual BT.601, uses `ranga::convert`, `ranga::color_correct`
- **aethersafta** -> drops custom alpha blend + color conversion, uses `ranga::blend`, `ranga::convert`

---

## When to Extract a Shared Crate

Extract when **3+ projects** implement the same pattern. Until then, keep it in-project.

Signs it's time to extract:
- You're copying a module between repos
- Two projects have different implementations of the same algorithm
- A bug fix in one project should automatically benefit another

---

*Last Updated: 2026-03-24*
