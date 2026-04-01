# Shared Crates — Registry & Status

> **Status**: Active | **Last Updated**: 2026-03-31
>
> **79 crates** — 45 at v1.0+ stable, 23 pre-1.0, 14 unpublished
>
> v1.0+ crate documentation lives in [docs/applications/libs/](../../applications/libs/).
> This file tracks **pre-1.0 crates**, **unpublished crates**, and the **stable crate index**.
> See [First-Party Standards](first-party-standards.md) for versioning and publishing conventions.

---

## v1.0+ Stable Index (45 crates)

Full documentation for each crate: [docs/applications/libs/{crate}.md](../../applications/libs/)

| Crate | Version | Domain |
|-------|---------|--------|
| abaco | 1.1.0 | Math engine |
| agnosai | 1.0.2 | AI orchestration |
| ai-hwaccel | 1.0.0 | GPU detection |
| badal | 1.1.0 | Weather/atmosphere |
| bhava | 1.7.0 | Emotion/personality |
| bijli | 1.1.0 | Electromagnetism |
| dhvani | 1.0.0 | Audio engine |
| dravya | 1.2.0 | Material science |
| garjan | 1.1.0 | Environmental sound |
| ghurni | 1.0.0 | Mechanical sound |
| goonj | 1.1.1 | Acoustics |
| hisab | 1.4.0 | Higher math |
| hoosh | 1.1.0 | LLM gateway |
| ifran | 1.2.0 | LLM inference/training |
| impetus | 1.3.0 | Physics |
| jantu | 1.1.0 | Ethology/behavior |
| kavach | 1.0.1 | Sandbox execution |
| khanij | 1.1.0 | Geology/mineralogy |
| kimiya | 1.1.1 | Chemistry |
| mabda | 1.0.0 | GPU foundation |
| majra | 1.0.3 | Queue/pub-sub |
| naad | 1.0.0 | Audio synthesis |
| nidhi | 1.1.0 | Sample playback |
| pavan | 1.1.0 | Aerodynamics |
| prakash | 1.2.0 | Optics/light |
| pramana | 1.1.0 | Statistics |
| prani | 1.1.0 | Creature vocals |
| pravash | 1.2.0 | Fluid dynamics |
| raasta | 1.0.0 | Pathfinding |
| shabda | 1.0.0 | G2P conversion |
| shabdakosh | 1.0.0 | Pronunciation dict |
| sharira | 1.0.0 | Physiology |
| shravan | 1.0.1 | Audio codecs |
| soorat | 1.0.0 | GPU rendering |
| stiva | 1.0.0 | Container runtime |
| svara | 1.1.0 | Vocal synthesis |
| szal | 1.0.1 | Workflow engine |
| tanmatra | 1.1.0 | Atomic physics |
| ushma | 1.3.0 | Thermodynamics |
| vanaspati | 1.0.0 | Botany |
| vidya | 1.0.0 | Programming reference |

---

## Pre-1.0 (21 crates)

### Infrastructure & OS

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [agnosys](https://github.com/MacCracken/agnosys) | 0.29.3 | Kernel interface — Landlock, seccomp, syscall bindings | daimon, aethersafha |
| [bote](https://github.com/MacCracken/bote) | 0.50.0 | MCP core — JSON-RPC 2.0, tool registry, dispatch | daimon, all MCP providers |
| [daimon](https://github.com/MacCracken/daimon) | 0.5.0 | Agent orchestrator — HTTP API, supervisor, IPC (port 8090) | all consumer apps |
| [libro](https://github.com/MacCracken/libro) | 0.25.3 | Cryptographic audit chain — hash-linked event logging | daimon, aegis, stiva |
| [nein](https://github.com/MacCracken/nein) | 0.24.3 | Programmatic nftables firewall — policy, NAT, port mapping | stiva, daimon, aegis |
| [phylax](https://github.com/MacCracken/phylax) | 0.5.0 | Threat detection — YARA, entropy, magic bytes, ML | daimon, aegis |
| [t-ron](https://github.com/MacCracken/t-ron) | 0.26.3 | MCP security — tool call auditing, rate limiting, injection detection | daimon, bote |
| [yukti](https://github.com/MacCracken/yukti) | 0.25.3 | Device abstraction — USB, block devices, udev hotplug | daimon, aethersafha |

### Graphics & Media

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [aethersafta](https://github.com/MacCracken/aethersafta) | 0.25.3 | Media compositing — scene graph, capture, HW encoding | aethersafha, tazama |
| [kiran](https://github.com/MacCracken/kiran) | 0.26.3 | Game engine — ECS, scheduling, scene hierarchy | joshua, salai |
| [muharrir](https://github.com/MacCracken/muharrir) | 0.23.5 | Editor primitives — text buffer, undo/redo, command pattern | rasa, tazama, shruti |
| [ranga](https://github.com/MacCracken/ranga) | 0.29.4 | Image processing — color spaces, blend modes, GPU compute | rasa, tazama, soorat |
| [selah](https://github.com/MacCracken/selah) | 0.29.4 | Screenshot capture, annotation, PII redaction | taswir, soorat |
| [tarang](https://github.com/MacCracken/tarang) | 0.21.3 | Media framework — container parsing, decode/encode | jalwa, tazama, shruti |

### Applications

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [jnana](https://github.com/MacCracken/jnana) | 0.5.0 | Unified knowledge system — offline-accessible corpus | agnoshi, hoosh, daimon |
| [joshua](https://github.com/MacCracken/joshua) | 0.1.0 | Game manager — AI NPCs, headless simulation | end user |
| [murti](https://github.com/MacCracken/murti) | 0.1.0 | Model runtime — registry, store, inference backends | hoosh, ifran, tanur |
| [salai](https://github.com/MacCracken/salai) | 0.1.0 | Game editor — egui visual editor for kiran | kiran |
| [tanur](https://github.com/MacCracken/tanur) | 0.1.0 | Desktop LLM studio — model management GUI | end user |

### Science (pre-1.0)

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [bodh](https://github.com/MacCracken/bodh) | 0.1.0 | Psychology — cognition, perception, learning | bhava, kiran, agnosai |
| [jivanu](https://github.com/MacCracken/jivanu) | 0.1.0 | Microbiology — growth kinetics, metabolism, genetics | sangha, kimiya, kiran |
| [lipi](https://github.com/MacCracken/lipi) | 0.1.0 | Multilingual language engine — phonemes, scripts, grammar | shabda, shabdakosh, svara, jnana |
| [mastishk](https://github.com/MacCracken/mastishk) | 0.1.0 | Neuroscience — neurotransmitters, sleep, HPA axis, DMN, chronobiology | bhava, bodh, kiran, joshua |
| [rasayan](https://github.com/MacCracken/rasayan) | 0.1.0 | Biochemistry — enzyme kinetics, metabolism, signal transduction, membrane transport | mastishk, sharira, jivanu, kimiya |
| [sangha](https://github.com/MacCracken/sangha) | 0.1.0 | Sociology — social networks, game theory, group dynamics | kiran, agnosai, bhava |

---

## Unpublished (14 crates)

Need `cargo publish` before consumers outside AGNOS can depend on them.

| Crate | Version | Description |
|-------|---------|-------------|
| bodh | 0.1.0 | Psychology engine |
| brahmanda | 0.1.0 | Galactic structure / large-scale cosmology |
| falak | 0.2.0 | Orbital mechanics |
| jivanu | 0.1.0 | Microbiology |
| jyotish | 0.1.0 | Astronomical computation |
| hisab-mimamsa | 0.1.0 | Theoretical physics (GR, cosmology, QFT) |
| kana | 0.1.0 | Quantum mechanics |
| lipi | 0.1.0 | Multilingual language engine (phonemes, scripts, grammar) |
| mastishk | 0.1.0 | Neuroscience (neurotransmitters, sleep, HPA, DMN, chronobiology) |
| phylax | 0.5.0 | Threat detection (crates.io stale at 0.22.3) |
| rasayan | 0.1.0 | Biochemistry (enzyme kinetics, metabolism, signal transduction) |
| sangha | 0.1.0 | Sociology |
| sankhya | 0.1.0 | Ancient mathematical systems |
| tara | 0.2.0 | Stellar astrophysics |

---

## GitHub Release Only (internal)

| Crate | Version | Description |
|-------|---------|-------------|
| [agnostik](https://github.com/MacCracken/agnostik) | 2026.3.26 | Shared types and domain primitives for AGNOS |

---

## Extraction Guidelines

Extract when **3+ projects** implement the same pattern. Until then, keep it in-project.

- You're copying a module between repos
- Two projects have different implementations of the same algorithm
- A bug fix in one project should automatically benefit another

See [monolith-extraction.md](../monolith-extraction.md) for the daimon/hoosh/agnoshi extraction plan.

See [k8s-roadmap.md](../k8s-roadmap.md) for stiva + nein + majra + kavach orchestration platform.

---

*Last Updated: 2026-03-31*
