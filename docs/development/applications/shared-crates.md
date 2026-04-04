# Shared Crates — Registry & Status

> **Status**: Active | **Last Updated**: 2026-04-03
>
> **77 crates** — 56 at v1.0+ stable (11 OS, 25 science, 10 media, 5 lang/nav, 5 physics), 20 pre-1.0, 1 internal
>
> v1.0+ crate documentation lives in [docs/applications/libs/](../../applications/libs/).
> This file tracks **pre-1.0 crates**, **unpublished crates**, and the **stable crate index**.
> See [First-Party Standards](first-party-standards.md) for versioning and publishing conventions.

---

## v1.0+ Stable Index (56 crates)

Full documentation for each crate: [docs/applications/libs/{crate}.md](../../applications/libs/)

### OS & Infrastructure (11 crates)

| Crate | Version | Domain |
|-------|---------|--------|
| agnosai | 1.0.2 | AI orchestration |
| ai-hwaccel | 1.0.0 | GPU detection |
| hoosh | 1.2.0 | LLM gateway |
| ifran | 1.2.0 | LLM inference/training |
| kavach | 2.0.0 | Sandbox execution |
| majra | 1.0.4 | Queue/pub-sub |
| sigil | 1.0.0 | Trust verification |
| stiva | 2.0.0 | Container runtime |
| szal | 1.0.1 | Workflow engine |
| mabda | 1.0.0 | GPU foundation |
| soorat | 1.0.0 | GPU rendering |

### Science & Knowledge (25 crates)

| Crate | Version | Domain |
|-------|---------|--------|
| abaco | 1.1.0 | Math engine |
| avatara | 1.0.1 | Divine archetype overlay |
| badal | 1.1.0 | Weather/atmosphere |
| bhava | 2.0.0 | Emotion/personality |
| bijli | 1.1.0 | Electromagnetism |
| bodh | 1.0.0 | Psychology |
| brahmanda | 1.0.0 | Galactic cosmology |
| dravya | 1.2.0 | Material science |
| falak | 1.0.0 | Orbital mechanics |
| hisab | 1.4.0 | Higher math |
| hisab-mimamsa | 1.0.0 | Theoretical physics |
| impetus | 1.3.0 | Physics |
| itihas | 1.0.1 | World history |
| jantu | 1.1.0 | Ethology/behavior |
| jivanu | 1.0.0 | Microbiology |
| jyotish | 1.0.0 | Astronomical computation |
| kana | 1.1.0 | Quantum mechanics |
| khanij | 1.1.0 | Geology/mineralogy |
| kimiya | 1.1.1 | Chemistry |
| mastishk | 1.0.0 | Neuroscience |
| pramana | 1.2.0 | Statistics |
| sangha | 1.0.0 | Sociology |
| sankhya | 1.0.0 | Ancient math systems |
| sharira | 1.0.0 | Physiology |
| vanaspati | 1.0.0 | Botany |

### Media & Audio (10 crates)

| Crate | Version | Domain |
|-------|---------|--------|
| dhvani | 1.0.0 | Audio engine |
| garjan | 1.1.0 | Environmental sound |
| ghurni | 1.0.0 | Mechanical sound |
| goonj | 1.1.1 | Acoustics |
| naad | 1.0.0 | Audio synthesis |
| nidhi | 1.1.0 | Sample playback |
| prani | 1.1.0 | Creature vocals |
| shravan | 1.0.1 | Audio codecs |
| svara | 1.1.1 | Vocal synthesis |
| shabda | 1.1.0 | G2P conversion |

### Language, Navigation & Reference (5 crates)

| Crate | Version | Domain |
|-------|---------|--------|
| raasta | 1.0.0 | Pathfinding |
| shabdakosh | 1.1.0 | Pronunciation dict |
| varna | 1.0.0 | Multilingual language engine |
| vidya | 1.0.0 | Programming reference |
| tara | 1.0.0 | Stellar astrophysics |

### Physics & Engineering (5 crates)

| Crate | Version | Domain |
|-------|---------|--------|
| pavan | 1.1.0 | Aerodynamics |
| prakash | 1.2.0 | Optics/light |
| pravash | 1.2.0 | Fluid dynamics |
| tanmatra | 1.2.1 | Atomic physics |
| ushma | 1.3.0 | Thermodynamics |

---

## Pre-1.0 (20 crates)

### Infrastructure & OS

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [agnosys](https://github.com/MacCracken/agnosys) | 0.51.0 | Kernel interface — Landlock, seccomp, syscall bindings | daimon, aethersafha, kavach, argonaut |
| [bote](https://github.com/MacCracken/bote) | 0.92.0 | MCP core — JSON-RPC 2.0, tool registry, host, dispatch | daimon, all MCP providers |
| [daimon](https://github.com/MacCracken/daimon) | 0.6.0 | Agent orchestrator — HTTP API, supervisor, IPC (port 8090) | all consumer apps |
| [libro](https://github.com/MacCracken/libro) | 0.92.0 | Cryptographic audit chain — hash-linked event logging (SHA-256, BLAKE3) | daimon, aegis, stiva, sigil, t-ron |
| [nein](https://github.com/MacCracken/nein) | 0.90.0 | Programmatic nftables firewall — policy, NAT, port mapping | stiva, daimon, aegis, kavach |
| [phylax](https://github.com/MacCracken/phylax) | 0.22.3 | Threat detection — YARA, entropy, magic bytes, ML | daimon, aegis |
| [t-ron](https://github.com/MacCracken/t-ron) | 0.90.0 | MCP security — tool call auditing, rate limiting, injection detection | daimon, bote |
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

### Science

| Crate | Version | Description | Key Consumers |
|-------|---------|-------------|---------------|
| [rasayan](https://github.com/MacCracken/rasayan) | 0.4.0 | Biochemistry — enzyme kinetics, metabolism, signal transduction, membrane transport | mastishk, sharira, jivanu, kimiya |

---

## Planned (2 crates)

Designed, not yet scaffolded.

| Crate | Planned Version | Description | Key Consumers |
|-------|----------------|-------------|---------------|
| **mudra** | 0.1.0 | Token/value primitives — asset identity, ownership, type, divisibility (Sanskrit: मुद्रा — coin, seal, token) | vinimaya, mela, aequi, bullshift |
| **vinimaya** | 0.1.0 | Transaction layer — atomic transfers, escrow, settlement, exchange (Sanskrit: विनिमय — exchange, barter). Lite mode for thin transaction layer. | mela, daimon, ark, seema, aequi, bullshift |
| **taal** | 0.1.0 | Music theory — scales, intervals, chords, rhythm, time signatures, key signatures, progressions, counterpoint (Sanskrit: ताल — rhythmic cycle) | naad, svara, shruti, jalwa |
| **natya** | 0.1.0 | Theater/drama/narrative — dramatic structure, character archetypes, rasa theory, comedy/tragedy, dialogue, narrative arcs (Sanskrit: नाट्य — drama, from the Natya Shastra) | bhava, agnoshi, hoosh, joshua |
| **kshetra** | 0.1.0 | Temporal geography — spatiotemporal database, (lat, lon, time) → state. Geology, climate, vegetation, settlement, political, hydrology layers (Sanskrit: क्षेत्र — field, domain) | itihas, badal, khanij, vanaspati, sangha, falak |

---

## Unpublished (0 crates)

Need `cargo publish` before consumers outside AGNOS can depend on them.

| Crate | Version | Description |
|-------|---------|-------------|

---

## System Binaries

Standalone executables — not library crates, but first-party AGNOS binaries tracked here for completeness.

| Binary | Version | Description | Depends On |
|--------|---------|-------------|------------|
| [kybernet](https://github.com/MacCracken/kybernet) | 0.51.0 | PID 1 init binary (extracted from argonaut) | argonaut |
| [shakti](https://github.com/MacCracken/shakti) | 0.1.0 | Privilege escalation (`sudo` replacement) | agnosys, sigil |

---

## GitHub Release Only (internal)

| Crate | Version | Description |
|-------|---------|-------------|
| [agnostik](https://github.com/MacCracken/agnostik) | 0.90.0 | Shared types and domain primitives for AGNOS |

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
