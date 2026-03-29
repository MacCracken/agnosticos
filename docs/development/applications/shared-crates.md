# Shared Crates — Ecosystem Infrastructure

> **Status**: Active | **Last Updated**: 2026-03-29 (versions verified against local repos)
>
> **63 library crates** — 34 at v1.0+ stable, 29 pre-1.0
> Standalone crates extracted from AGNOS that the entire ecosystem depends on.
> Published to crates.io or distributed via GitHub Releases.
> See [First-Party Standards](first-party-standards.md) for versioning and publishing conventions.
> App documentation for each crate is in [docs/applications/libs/](../../applications/libs/).

---

## Registry — v1.0+ Stable (34 crates)

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [hisab](https://github.com/MacCracken/hisab) | 1.3.0 | Higher math — linear algebra, calculus, geometry, spatial structures, ODE solvers, FFT | all science crates, impetus, kiran, soorat |
| [bhava](https://github.com/MacCracken/bhava) | 1.2.0 | Emotion/personality — 15-trait personalities, PAD mood vectors, archetypes, sentiment | SY, joshua, agnosai, kiran |
| [prakash](https://github.com/MacCracken/prakash) | 1.2.0 | Optics/light — ray optics, wave optics, spectral math, PBR, atmospheric scattering | soorat, kiran, tara |
| [impetus](https://github.com/MacCracken/impetus) | 1.2.0 | Physics — 2D/3D rigid bodies, collision detection, constraints, spatial queries | kiran, joshua, pavan |
| [ushma](https://github.com/MacCracken/ushma) | 1.1.0 | Thermodynamics — heat transfer, entropy, equations of state, thermal properties | kimiya, kiran, badal |
| [pravash](https://github.com/MacCracken/pravash) | 1.1.0 | Fluid dynamics — SPH, Euler/Navier-Stokes, shallow water, buoyancy, vortex | pavan, badal, kiran |
| [goonj](https://github.com/MacCracken/goonj) | 1.1.0 | Acoustics — sound propagation, room simulation, impulse response generation | dhvani, kiran, joshua |
| [hoosh](https://github.com/MacCracken/hoosh) | 1.1.0 | AI inference gateway — 15 LLM providers, token budget management (port 8088) | daimon, all consumer apps |
| [ifran](https://github.com/MacCracken/ifran) | 1.1.0 | Local LLM inference — 16 backends, training, fleet management, REST+gRPC | hoosh, murti, tanur |
| [abaco](https://github.com/MacCracken/abaco) | 1.1.0 | Math engine — expression evaluation, unit conversion, numeric types | abacus, dhvani |
| [svara](https://github.com/MacCracken/svara) | 1.1.0 | Formant & vocal synthesis — glottal source, 48 phonemes, coarticulation | dhvani, vansh, prani |
| [prani](https://github.com/MacCracken/prani) | 1.1.0 | Creature vocal synthesis — 13 species, dual syrinx, subharmonics, emotion | kiran, joshua, dhvani |
| [nidhi](https://github.com/MacCracken/nidhi) | 1.1.0 | Sample playback — polyphonic sampler, SFZ/SF2, key/velocity zones | dhvani, shruti |
| [bijli](https://github.com/MacCracken/bijli) | 1.0.1 | Electromagnetism — fields, Maxwell, FDTD, Fresnel, scattering | prakash, kiran |
| [kavach](https://github.com/MacCracken/kavach) | 1.0.1 | Sandbox execution — 8 backends, strength scoring, policy engine | daimon, stiva, kiran |
| [szal](https://github.com/MacCracken/szal) | 1.0.1 | Workflow engine — step/flow execution, branching, retry, rollback | daimon, sutra |
| [majra](https://github.com/MacCracken/majra) | 1.0.2 | Distributed queue & multiplex — pub/sub, priority queues, heartbeat | daimon, stiva, sutra |
| [agnosai](https://github.com/MacCracken/agnosai) | 1.0.2 | Provider-agnostic AI orchestration — crews, task DAGs, tool execution | agnostic, daimon, joshua |
| [shravan](https://github.com/MacCracken/shravan) | 1.0.1 | Audio codecs — WAV, FLAC, AIFF, Ogg, MP3, Opus, PCM, resampling | dhvani, shruti, jalwa, tarang |
| [ai-hwaccel](https://github.com/MacCracken/ai-hwaccel) | 1.0.0 | Universal AI hardware accelerator detection, capability querying | hoosh, daimon, kiran |
| [kimiya](https://github.com/MacCracken/kimiya) | 1.0.0 | Chemistry — elements, molecules, reactions, kinetics, thermochemistry | khanij, tara |
| [stiva](https://github.com/MacCracken/stiva) | 1.0.0 | OCI container runtime — image management, container lifecycle | daimon, sutra |
| [pavan](https://github.com/MacCracken/pavan) | 1.0.0 | Aerodynamics — atmosphere, airfoils, panel methods, compressible flow | kiran, joshua |
| [dravya](https://github.com/MacCracken/dravya) | 1.0.0 | Material science — stress, strain, elasticity, fatigue, composites | impetus, kiran |
| [badal](https://github.com/MacCracken/badal) | 1.0.0 | Weather/atmospheric modeling — weather simulation, atmospheric dynamics | kiran, joshua, pavan |
| [khanij](https://github.com/MacCracken/khanij) | 1.0.0 | Geology/mineralogy — crystal structures, rocks, soil, geochemistry | kiran, joshua |
| [naad](https://github.com/MacCracken/naad) | 1.0.0 | Audio synthesis — oscillators, filters, envelopes, modulation, wavetables | svara, nidhi, dhvani |
| [shabdakosh](https://github.com/MacCracken/shabdakosh) | 1.0.0 | Pronunciation dictionary — ARPABET/IPA, CMUdict 10K+ entries, user overlays | shabda, dhvani, vansh |
| [shabda](https://github.com/MacCracken/shabda) | 1.0.0 | Grapheme-to-phoneme — text normalization, tokenization, G2P rules, prosody | svara, dhvani, vansh |
| [garjan](https://github.com/MacCracken/garjan) | 1.0.0 | Environmental sound — thunder, rain, wind, fire, impacts, ambient textures | kiran, joshua, dhvani |
| [ghurni](https://github.com/MacCracken/ghurni) | 1.0.0 | Mechanical sound — engines, gears, motors, turbines, drivetrain | kiran, joshua, dhvani |
| [tanmatra](https://github.com/MacCracken/tanmatra) | 1.0.0 | Atomic/subatomic — Standard Model, nuclear structure, decay, spectral lines | kimiya, kiran |
| [pramana](https://github.com/MacCracken/pramana) | 1.0.0 | Statistics & probability — distributions, Bayesian, hypothesis testing, MCMC | all science crates |
| [jantu](https://github.com/MacCracken/jantu) | 1.0.0 | Ethology/creature behavior — instinct, survival, swarm intelligence | kiran, joshua |

## Registry — Pre-1.0 (29 crates)

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [bote](https://github.com/MacCracken/bote) | 0.50.0 | MCP core service — JSON-RPC 2.0, tool registry, dispatch | daimon, all MCP providers |
| [ranga](https://github.com/MacCracken/ranga) | 0.29.4 | Core image processing — color spaces, blend modes, pixel buffers, GPU compute | rasa, tazama, soorat |
| [selah](https://github.com/MacCracken/selah) | 0.29.4 | Screenshot capture, annotation, PII redaction | taswir, soorat, aethersafta |
| [soorat](https://github.com/MacCracken/soorat) | 0.29.3 | GPU rendering — wgpu, 2D/3D PBR, shadows, animation, post-processing | kiran, salai, joshua |
| [kiran](https://github.com/MacCracken/kiran) | 0.26.3 | Game engine — ECS, system scheduling, scene hierarchy, 9 integrations | joshua, salai |
| [raasta](https://github.com/MacCracken/raasta) | 0.26.3 | Navigation/pathfinding — A*, JPS, HPA*, navmesh, crowd simulation | kiran, joshua |
| [t-ron](https://github.com/MacCracken/t-ron) | 0.26.3 | MCP security monitor — tool call auditing, rate limiting, injection detection | daimon, bote |
| [aethersafta](https://github.com/MacCracken/aethersafta) | 0.25.3 | Real-time media compositing — scene graph, multi-source capture, HW encoding | aethersafha, tazama |
| [libro](https://github.com/MacCracken/libro) | 0.25.3 | Cryptographic audit chain — tamper-proof hash-linked event logging | daimon, aegis, stiva |
| [yukti](https://github.com/MacCracken/yukti) | 0.25.3 | Device abstraction — USB, optical, block devices, udev hotplug, mount/eject | daimon, aethersafha |
| [nein](https://github.com/MacCracken/nein) | 0.24.3 | Programmatic nftables firewall — network policy, NAT, port mapping | stiva, daimon, aegis |
| [muharrir](https://github.com/MacCracken/muharrir) | 0.23.5 | Shared editor primitives — text buffer, undo/redo, command pattern | rasa, tazama, shruti |
| [dhvani](https://github.com/MacCracken/dhvani) | 0.22.4 | Core audio engine — buffers, DSP, resampling, mixing, analysis | shruti, jalwa, kiran |
| [tarang](https://github.com/MacCracken/tarang) | 0.21.3 | AI-native media framework — container parsing, audio/video decode/encode | jalwa, tazama, shruti |
| [phylax](https://github.com/MacCracken/phylax) | 0.5.0 | Threat detection — YARA rules, entropy analysis, magic bytes, ML | daimon, aegis |
| [agnosys](https://github.com/MacCracken/agnosys) | 0.5.0 | Kernel interface — safe Rust bindings for Linux syscalls, Landlock, seccomp | daimon, aethersafha |
| [daimon](https://github.com/MacCracken/daimon) | 0.5.0 | Agent orchestrator — HTTP API, supervisor, IPC, scheduler (port 8090) | all consumer apps |
| [mabda](https://github.com/MacCracken/mabda) | 0.1.0 | GPU foundation — device, buffers, compute dispatch, textures, profiling | soorat, rasa, ranga, bijli |
| [murti](https://github.com/MacCracken/murti) | 0.1.0 | Core model runtime — registry, store, pull, inference backends | hoosh, ifran, tanur |
| [tanur](https://github.com/MacCracken/tanur) | 0.1.0 | Desktop LLM studio — model management, training, inference GUI | end user |
| [joshua](https://github.com/MacCracken/joshua) | 0.1.0 | Game manager — AI NPCs, headless simulation, deterministic replay | end user |
| [salai](https://github.com/MacCracken/salai) | 0.1.0 | Game editor — egui visual editor for kiran | kiran |
| [kana](https://github.com/MacCracken/kana) | 0.1.0 | Quantum mechanics — state vectors, operators, entanglement, circuits | tanmatra |
| [falak](https://github.com/MacCracken/falak) | 0.1.0 | Orbital mechanics — Keplerian orbits, transfers, perturbations | jyotish, tara |
| [jyotish](https://github.com/MacCracken/jyotish) | 0.1.0 | Astronomical computation — planetary positions, calendar systems | kiran, joshua |
| [tara](https://github.com/MacCracken/tara) | 0.1.0 | Stellar astrophysics — classification, evolution, nucleosynthesis | falak, jyotish |
| [sankhya](https://github.com/MacCracken/sankhya) | 0.1.0 | Ancient mathematical systems — Mayan, Babylonian, Egyptian, Vedic, Chinese | educational |
| [vanaspati](https://github.com/MacCracken/vanaspati) | 0.1.0 | Botany — plant growth, photosynthesis, root systems, pollination | kiran, joshua |
| [sharira](https://github.com/MacCracken/sharira) | 0.1.0 | Physiology — skeletal, muscular, respiratory, cardiovascular systems | kiran, joshua |
| [vidya](https://github.com/MacCracken/vidya) | 0.1.0 | Programming reference library — multi-language best practices, queryable corpus | agnoshi, hoosh, daimon |

## GitHub Release Only (internal to AGNOS)

| Crate | Version | Description |
|-------|---------|-------------|
| [agnostik](https://github.com/MacCracken/agnostik) | 2026.3.26 | Shared types, error handling, and domain primitives for AGNOS |

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

*Last Updated: 2026-03-29*
