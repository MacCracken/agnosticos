# Shared Crates — Ecosystem Infrastructure

> **Status**: Active | **Last Updated**: 2026-03-27
>
> Standalone crates extracted from AGNOS that the entire ecosystem depends on.
> Published to crates.io or distributed via GitHub Releases.
> See [First-Party Standards](first-party-standards.md) for versioning and publishing conventions.
> App documentation for each crate is in [docs/applications/](../../applications/).

---

## Registry — 1.0+ Stable (19 crates)

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [hisab](https://github.com/MacCracken/hisab) | 1.2.0 | Higher math — linear algebra, calculus, geometry, spatial structures (BVH, k-d tree, octree, GJK/EPA), ODE solvers, FFT, compensated summation | all science crates, impetus, kiran, soorat, raasta, svara, prani |
| [bhava](https://github.com/MacCracken/bhava) | 1.1.0 | Emotion/personality — 15-trait personalities, PAD mood vectors, archetypes, sentiment, circadian, EQ, micro-expressions | SY, joshua, agnosai, kiran |
| [prakash](https://github.com/MacCracken/prakash) | 1.1.0 | Optics/light — ray optics, wave optics, spectral math, lens geometry, PBR, atmospheric scattering | soorat, kiran, tara |
| [impetus](https://github.com/MacCracken/impetus) | 1.1.0 | Physics — 2D/3D rigid bodies, collision detection, constraints, spatial queries | kiran, joshua, pavan |
| [ushma](https://github.com/MacCracken/ushma) | 1.1.0 | Thermodynamics — heat transfer, entropy, equations of state, thermal properties, cycles | kimiya, kiran, badal |
| [pravash](https://github.com/MacCracken/pravash) | 1.1.0 | Fluid dynamics — SPH, Euler/Navier-Stokes, shallow water, buoyancy, drag, vortex | pavan, badal, kiran |
| [kimiya](https://github.com/MacCracken/kimiya) | 1.0.0 | Chemistry — elements, molecules, reactions, kinetics, thermochemistry | khanij, tara |
| [kavach](https://github.com/MacCracken/kavach) | 1.0.1 | Sandbox execution — 8 backends, strength scoring, policy engine, credential proxy | daimon, stiva, SY, kiran |
| [stiva](https://github.com/MacCracken/stiva) | 1.0.0 | OCI container runtime — image management, container lifecycle, orchestration | daimon, sutra |
| [bijli](https://github.com/MacCracken/bijli) | 1.0.0 | Electromagnetism — fields, Maxwell's equations, charge dynamics, EM waves | prakash, kiran |
| [goonj](https://github.com/MacCracken/goonj) | 1.0.0 | Acoustics — sound propagation, room simulation, impulse response generation | dhvani, kiran, joshua |
| [pavan](https://github.com/MacCracken/pavan) | 1.0.0 | Aerodynamics — atmosphere, airfoils, panel methods, VLM, compressible flow | kiran, joshua |
| [dravya](https://github.com/MacCracken/dravya) | 1.0.0 | Material science — stress, strain, elasticity, fatigue, fracture, composites | impetus, kiran |
| [badal](https://github.com/MacCracken/badal) | 1.0.0 | Weather/atmospheric modeling — weather simulation, atmospheric dynamics | kiran, joshua, pavan |
| [khanij](https://github.com/MacCracken/khanij) | 1.0.0 | Geology/mineralogy — crystal structures, rock cycles, soil, mineral properties, geochemistry | kiran, joshua |
| [naad](https://github.com/MacCracken/naad) | 1.0.0 | Audio synthesis — oscillators, filters, envelopes, modulation, wavetables, effects | svara, dhvani, kiran |
| [svara](https://github.com/MacCracken/svara) | 1.0.0 | Formant & vocal synthesis — glottal source (Rosenberg/LF), SOA formant bank, 48 phonemes, coarticulation, spectral analysis | dhvani, vansh, prani, shabda |

## Registry — Published on crates.io (21 pre-1.0 crates)

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [kiran](https://github.com/MacCracken/kiran) | 0.26.3 | Game engine — ECS, system scheduling, scene hierarchy, audio, physics, scripting | joshua, salai |
| [ai-hwaccel](https://github.com/MacCracken/ai-hwaccel) | 0.23.3 | Universal AI hardware accelerator detection, capability querying, workload planning | hoosh, daimon, kiran |
| [tarang](https://github.com/MacCracken/tarang) | 0.21.3 | AI-native media framework — container parsing, audio/video decode, encode, mux, fingerprint | jalwa, tazama, shruti, aethersafta |
| [ranga](https://github.com/MacCracken/ranga) | 0.24.3 | Core image processing — color spaces, blend modes, pixel buffers, filters, GPU compute | rasa, tazama, aethersafta |
| [dhvani](https://github.com/MacCracken/dhvani) | 0.22.4 | Core audio engine — buffers, DSP, resampling, mixing, analysis, capture | shruti, jalwa, kiran, goonj |
| [hoosh](https://github.com/MacCracken/hoosh) | 0.25.3 | AI inference gateway — 15 LLM providers, local model serving, token budget management (port 8088) | daimon, all consumer apps |
| [aethersafta](https://github.com/MacCracken/aethersafta) | 0.25.3 | Real-time media compositing — scene graph, multi-source capture, HW encoding | aethersafha, tazama, SY |
| [majra](https://github.com/MacCracken/majra) | 0.22.3 | Distributed queue & multiplex — pub/sub, priority queues, heartbeat, relay, IPC | daimon, stiva, AgnosAI, sutra |
| [agnosai](https://github.com/MacCracken/agnosai) | 0.25.3 | Provider-agnostic AI orchestration — crews, task DAGs, tool execution | agnostic, daimon, joshua |
| [libro](https://github.com/MacCracken/libro) | 0.25.3 | Cryptographic audit chain — tamper-proof hash-linked event logging | daimon, aegis, stiva |
| [bote](https://github.com/MacCracken/bote) | 0.22.3 | MCP core service — JSON-RPC 2.0, tool registry, dispatch | daimon, all MCP tool providers |
| [nein](https://github.com/MacCracken/nein) | 0.24.3 | Programmatic nftables firewall — network policy, NAT, port mapping | stiva, daimon, aegis |
| [szal](https://github.com/MacCracken/szal) | 0.23.4 | Workflow engine — step/flow execution, branching, retry, rollback, parallel stages | daimon, sutra |
| [muharrir](https://github.com/MacCracken/muharrir) | 0.23.5 | Shared editor primitives — text buffer, undo/redo, syntax highlighting | rasa, tazama, shruti, salai |
| [yukti](https://github.com/MacCracken/yukti) | 0.25.3 | Device abstraction — USB, optical, block devices, udev hotplug, mount/eject | daimon, aethersafha, jalwa |
| [phylax](https://github.com/MacCracken/phylax) | 0.22.3 | Threat detection — YARA rules, entropy analysis, magic bytes, ML classification | daimon, aegis |
| [selah](https://github.com/MacCracken/selah) | 0.24.3 | Screenshot capture, annotation, PII redaction | taswir, aethersafta |
| [raasta](https://github.com/MacCracken/raasta) | 0.26.3 | Navigation/pathfinding — A*, JPS, HPA*, navmesh, crowd simulation | kiran, joshua |
| [soorat](https://github.com/MacCracken/soorat) | 0.24.3 | GPU rendering — wgpu, 2D/3D PBR, shadows, animation, post-processing | kiran, joshua |
| [t-ron](https://github.com/MacCracken/t-ron) | 0.22.4 | MCP security monitor — tool call auditing, rate limiting, injection detection | daimon, bote |
| [abaco](https://github.com/MacCracken/abaco) | 0.22.4 | Math engine — expression evaluation, unit conversion, numeric types | abacus, dhvani |

## GitHub Release Only (internal to AGNOS)

| Crate | Version | Description |
|-------|---------|-------------|
| [agnostik](https://github.com/MacCracken/agnostik) | 2026.3.26 | Shared types, error handling, and domain primitives for AGNOS |
| [agnosys](https://github.com/MacCracken/agnosys) | 0.25.4 | Kernel interface — safe Rust bindings for Linux syscalls, Landlock, seccomp, udev, TPM |

## Not Yet Published

| Crate | Status | Description |
|-------|--------|-------------|
| [murti](https://github.com/MacCracken/murti) | Scaffolded | Core model runtime — registry, store, pull, inference backends |
| [kana](https://github.com/MacCracken/kana) | Scaffolded | Quantum mechanics — state vectors, operators, entanglement, circuits |
| [salai](https://github.com/MacCracken/salai) | Scaffolded | Game editor — egui visual editor for kiran |
| [prani](https://github.com/MacCracken/prani) | v1.0-ready | Creature vocal synthesis — 13 species, dual syrinx, subharmonics, dragon fire-breath, pitch contours |
| [shabda](https://github.com/MacCracken/shabda) | Scaffolded | Grapheme-to-phoneme (G2P) — text to phoneme sequences, pronunciation dictionary, English rules |
| [garjan](https://github.com/MacCracken/garjan) | Scaffolded | Environmental sound synthesis — thunder, rain, wind, fire, impacts, water, ambient textures |
| [ghurni](https://github.com/MacCracken/ghurni) | Scaffolded | Mechanical sound synthesis — engines, gears, motors, turbines, clocks, RPM-driven harmonics |
| [pramana](https://github.com/MacCracken/pramana) | Scaffolded | Statistics & probability — distributions, Bayesian, hypothesis testing, Monte Carlo, Markov chains |
| [sankhya](https://github.com/MacCracken/sankhya) | Scaffolded | Ancient mathematical systems — Mayan, Babylonian, Egyptian, Vedic, Chinese, Greek |

## Scaffolded (newly created)

| Crate | Description |
|-------|-------------|
| [tara](https://github.com/MacCracken/tara) | Stellar astrophysics — star classification, evolution, nucleosynthesis, spectral analysis |
| [falak](https://github.com/MacCracken/falak) | Orbital mechanics — Keplerian orbits, perturbations, transfers, celestial mechanics |
| [jyotish](https://github.com/MacCracken/jyotish) | Astronomical computation — planetary positions, calendar systems, celestial events |
| [joshua](https://github.com/MacCracken/joshua) | Game manager — AI NPCs, headless simulation, deterministic replay |
| [daimon](https://github.com/MacCracken/daimon) | Agent orchestrator — HTTP API, supervisor, IPC, scheduler, federation (port 8090) |

---

## Science Stack

Built on hisab for math, each owning a specific domain of physical simulation:

| Crate | Etymology | Domain | Status |
|-------|-----------|--------|--------|
| [prakash](https://github.com/MacCracken/prakash) | Sanskrit: प्रकाश (light) | Optics — ray tracing, wave optics, spectral math, PBR | **1.1.0** |
| [bijli](https://github.com/MacCracken/bijli) | Hindi: बिजली (electricity) | Electromagnetism — fields, Maxwell, FDTD, Fresnel, scattering | **1.0.0** |
| [ushma](https://github.com/MacCracken/ushma) | Sanskrit: ऊष्मा (heat) | Thermodynamics — heat transfer, entropy, equations of state | **1.1.0** |
| [pravash](https://github.com/MacCracken/pravash) | Sanskrit: प्रवाह (flow) | Fluid dynamics — SPH, Navier-Stokes, shallow water | **1.1.0** |
| [kimiya](https://github.com/MacCracken/kimiya) | Arabic: كيمياء (alchemy) | Chemistry — elements, reactions, kinetics, thermochemistry | **1.0.0** |
| [goonj](https://github.com/MacCracken/goonj) | Hindi: गूँज (echo) | Acoustics — room simulation, impulse responses, diffraction | **1.0.0** |
| [pavan](https://github.com/MacCracken/pavan) | Sanskrit: पवन (wind) | Aerodynamics — atmosphere, airfoils, panel methods, compressible flow | **1.0.0** |
| [dravya](https://github.com/MacCracken/dravya) | Sanskrit: द्रव्य (substance) | Material science — stress/strain, elasticity, fatigue, composites | **1.0.0** |
| [badal](https://github.com/MacCracken/badal) | Hindi: बादल (cloud) | Weather/atmospheric — weather simulation, atmospheric dynamics | **1.0.0** |
| [khanij](https://github.com/MacCracken/khanij) | Hindi/Sanskrit: खनिज (mineral) | Geology/mineralogy — crystals, rocks, soil, geochemistry | **1.0.0** |
| [kana](https://github.com/MacCracken/kana) | Sanskrit: कण (particle) | Quantum mechanics — state vectors, operators, entanglement | Scaffolded |
| [tara](https://github.com/MacCracken/tara) | Sanskrit: तारा (star) | Stellar astrophysics — classification, evolution, nucleosynthesis | Scaffolded |
| [falak](https://github.com/MacCracken/falak) | Arabic/Persian: فلک (sky) | Orbital mechanics — Keplerian orbits, transfers, perturbations | Scaffolded |
| [jyotish](https://github.com/MacCracken/jyotish) | Sanskrit: ज्योतिष (light) | Astronomical computation — planetary positions, calendar systems | Scaffolded |
| [jantu](https://github.com/MacCracken/jantu) | Sanskrit: जन्तु (creature) | Ethology/creature behavior — instinct, survival, swarm intelligence | **1.0.0** |
| [naad](https://github.com/MacCracken/naad) | Sanskrit: नाद (primordial sound) | Audio synthesis — oscillators, filters, envelopes, wavetables, effects | **1.0.0** |
| [svara](https://github.com/MacCracken/svara) | Sanskrit: स्वर (voice/tone) | Formant & vocal synthesis — glottal source (Rosenberg/LF), SOA formant bank, 48 phonemes, ~1000x RT | **1.0.0** |
| [shabda](https://github.com/MacCracken/shabda) | Sanskrit: शब्द (word/sound) | Grapheme-to-phoneme — text to phoneme sequences, dictionary + rules, prosody mapping | Scaffolded |
| [prani](https://github.com/MacCracken/prani) | Sanskrit: प्राणी (living being) | Creature vocal synthesis — 13 species, dual syrinx, subharmonics, ~700x RT | v1.0-ready |
| [garjan](https://github.com/MacCracken/garjan) | Sanskrit: गर्जन (roar/thunder) | Environmental sound synthesis — weather, impacts, water, fire, ambient textures | Scaffolded |
| [ghurni](https://github.com/MacCracken/ghurni) | Sanskrit: घूर्णि (rotation/spinning) | Mechanical sound synthesis — engines, gears, motors, turbines, clocks | Scaffolded |
| nada-brahma | Sanskrit: नाद ब्रह्म (universe is sound) | Cosmic sonification — stellar oscillations, orbital harmonics, celestial event mapping | Planned |
| [pramana](https://github.com/MacCracken/pramana) | Sanskrit: प्रमाण (proof/evidence) | Statistics & probability — distributions, Bayesian, hypothesis testing, Monte Carlo, Markov | Scaffolded |
| [sankhya](https://github.com/MacCracken/sankhya) | Sanskrit: सांख्य (enumeration) | Ancient mathematical systems — Mayan, Babylonian, Egyptian, Vedic, Chinese, Greek | Scaffolded |

---

## When to Extract a Shared Crate

Extract when **3+ projects** implement the same pattern. Until then, keep it in-project.

Signs it's time to extract:
- You're copying a module between repos
- Two projects have different implementations of the same algorithm
- A bug fix in one project should automatically benefit another

---

See [k8s-roadmap.md](../k8s-roadmap.md) for how stiva + nein + majra + kavach compose into a k8s-equivalent orchestration platform.

See [monolith-extraction.md](../monolith-extraction.md) for the plan to extract daimon, hoosh, agnoshi, and aethersafha from the monolithic userland workspace.

---

*Last Updated: 2026-03-27*
