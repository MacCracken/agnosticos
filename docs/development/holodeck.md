# Holodeck — Immersive Simulation Architecture

> **Status**: Vision | **Last Updated**: 2026-04-03
>
> A fully immersive, AI-driven simulation environment built on existing AGNOS subsystems.
> Not a VR headset application — a room-scale interactive environment where the intelligence,
> narrative, physics, and audio are native to the OS, not bolted on.

---

## What the Holodeck Is

An enclosed environment where a user interacts with AI-generated characters, objects, and scenarios that are perceptually indistinguishable from reality. The simulation is driven by real-time intelligence — not pre-scripted sequences — with characters that have emotional states, narrative purpose, and autonomous decision-making.

The holodeck is not a game engine with better graphics. It is a **sovereign simulation** — every layer from narrative generation to physics to rendering to audio is controlled by AGNOS subsystems, not third-party middleware.

---

## Existing AGNOS Coverage

The software architecture for a holodeck is substantially covered by existing crates. The gaps are primarily in hardware interfaces and physical-layer technology.

### Rendering & Visual

| Need | AGNOS Crate | Version | Coverage | Gap |
|------|-------------|---------|----------|-----|
| Scene graph & compositing | **aethersafta** | 0.25.3 | Scene graph, z-ordered layers, alpha blending, frame-accurate mixing | Currently 2D-oriented. Needs 3D scene graph extension |
| Desktop compositing | **aethersafha** | 0.1.0 | Wayland compositor, surface management, display output | 2D surfaces. Holodeck needs volumetric output pipeline |
| GPU rendering | **soorat** | 1.0.0 | GPU rendering primitives | Needs real-time raytracing, light field rendering |
| Game engine / ECS | **kiran** | 0.26.3 | Entity-Component-System, scheduling, scene hierarchy | Core architecture present. Needs physics integration, LOD, spatial partitioning at room scale |
| Image processing | **ranga** | 0.29.4 | Color spaces, blend modes, GPU compute | Texture generation, material synthesis for holographic surfaces |
| GPU hardware | **ai-hwaccel** | 1.0.0 | 13 GPU/TPU families detected | Present — routes to available hardware |
| **Hardware gap** | — | — | — | Volumetric display, holographic projection, or light field arrays. No software solution — requires physical display technology |

### Audio

| Need | AGNOS Crate | Version | Coverage | Gap |
|------|-------------|---------|----------|-----|
| Audio synthesis | **naad** | 1.0.0 | Oscillators, filters, envelopes, wavetable, FM, effects | Fully covered for procedural audio |
| Vocal synthesis | **svara** | 1.1.1 | Glottal pulse, formant model, IPA phonemes, prosody | NPC voice generation covered |
| Audio engine | **dhvani** | 1.0.0 | DSP, mixing, synthesis, PipeWire integration | Core engine present |
| Acoustics | **goonj** | 1.1.1 | Room acoustics, reverb modeling | Present — needs real-time HRTF for spatial audio |
| Environmental sound | **garjan** | 1.1.0 | Environmental/atmospheric sound | Ambient soundscapes covered |
| Audio codecs | **shravan** | 1.0.1 | Encode/decode | Present |
| Speech processing | **shabda** | 1.1.0 | G2P, speech-to-text pipeline | User speech input covered |
| **Hardware gap** | — | — | — | Waveguide speaker arrays for true spatial audio, bone conduction for private channels, ultrasonic parametric speakers for directional sound |

### Intelligence & Characters

| Need | AGNOS Crate | Version | Coverage | Gap |
|------|-------------|---------|----------|-----|
| NPC intelligence | **hoosh** | 1.2.0 | 15 LLM providers, inference routing, token budgets | Fully covered — NPCs are just agents with hoosh backing |
| Emotional state | **bhava** | 2.0.0 | Emotion modeling, personality, mood | NPC emotional realism covered |
| Narrative structure | **natya** | 0.1.0 | Dramatic arcs, character archetypes, rasa, beats, dialogue | Story generation framework present |
| Character archetypes | **natya** | 0.1.0 | 8 Jungian/Campbell archetypes with dramatic functions | NPC role assignment covered |
| Agent orchestration | **daimon** | 0.6.0 | Agent lifecycle, IPC, 144 MCP tools | NPCs as orchestrated agents — fully covered |
| Agent communication | **bote** | 0.92.0 | MCP JSON-RPC 2.0, tool registry | Agent-to-agent communication covered |
| Psychology | **bodh** | 1.0.0 | Psychological modeling | NPC psychological depth |
| Sociology | **sangha** | 1.0.0 | Social dynamics modeling | Group NPC behavior, crowd dynamics |
| World history | **itihas** | 1.0.1 | Historical knowledge base | Historical scenario generation |
| **Gap: none** | — | — | The NPC/intelligence layer is the most complete. An NPC is a daimon agent with bhava + natya + hoosh. It doesn't know it's in a simulation. | — |

### Physics & Environment

| Need | AGNOS Crate | Version | Coverage | Gap |
|------|-------------|---------|----------|-----|
| Classical physics | **impetus** | 1.3.0 | Mechanics, forces, collision | Core physics present. Needs real-time constraint solver at room scale |
| Fluid dynamics | **pravash** | 1.2.0 | Fluid simulation | Water, smoke, fog effects |
| Aerodynamics | **pavan** | 1.1.0 | Wind, airflow | Environmental wind, cloth simulation |
| Optics/light | **prakash** | 1.2.0 | Light behavior, refraction, reflection | Lighting model for rendered environment |
| Thermodynamics | **ushma** | 1.3.0 | Heat, temperature | Environmental temperature, fire simulation |
| Electromagnetism | **bijli** | 1.1.0 | EM fields | Lightning, electrical effects |
| Chemistry | **kimiya** | 1.1.1 | Chemical reactions | Material interactions, fire chemistry |
| Materials | **dravya** | 1.2.0 | Material properties | Surface properties for rendered objects |
| Atomic physics | **tanmatra** | 1.2.1 | Atomic-level modeling | Material behavior at fine grain |
| **Gap** | — | — | — | Real-time physics solver that unifies these crates into a single simulation loop running at ≥90 FPS. Individual crates exist but the unified simulation runtime does not |

### Narrative & Experience

| Need | AGNOS Crate | Version | Coverage | Gap |
|------|-------------|---------|----------|-----|
| Story generation | **natya** | 0.1.0 | Dramatic arcs (3-act, 5-act), beats, phases | Framework present |
| Emotional aesthetics | **natya** | 0.1.0 | 9 rasas from Natya Shastra | Emotional targeting for scenes |
| Dialogue generation | **natya** | 0.1.0 | DialogueLine, Exchange types | Structure present, content via hoosh |
| Music theory | **taal** | 0.1.0 | Scales, chords, rhythm, tempo, key signatures | Soundtrack generation framework |
| Music synthesis | **naad** + **svara** | 1.0.0 / 1.1.1 | Full audio synthesis pipeline | Dynamic soundtrack possible |
| User intent parsing | **agnoshi** | 0.90.0 | Natural language → intent | "Computer, create a forest scene" → parsed intent |
| Workflow orchestration | **szal** | 1.1.0 | Branching, retry, rollback | Scene transition orchestration |
| **Gap** | — | — | — | Real-time narrative director — an agent that monitors the user's emotional state, adjusts story pacing, manages NPC coordination. Sits above natya and below hoosh. Not yet designed |

### Security & Safety

| Need | AGNOS Crate | Version | Coverage | Gap |
|------|-------------|---------|----------|-----|
| Simulation sandboxing | **kavach** | 2.0.0 | 8 sandbox backends, strength scoring | Critical — the simulation must not escape its boundary |
| Security policy | **aegis** | 0.1.0 | System hardening, policy enforcement | Safety interlocks for the physical environment |
| Threat detection | **phylax** | 0.5.0 | YARA, ML analysis | Detect adversarial content in generated scenes |
| Audit trail | **libro** | 0.92.0 | Tamper-proof event logging | Record everything that happens in simulation |
| Trust verification | **sigil** | 1.0.0 | Ed25519 signing | Verify scene packages, NPC identities |
| Firewall | **nein** | 0.90.0 | Network policy | Isolate simulation network traffic |
| **Hardware gap** | — | — | — | Physical safety interlocks — emergency stop, environmental sensors (temperature, CO2, user biometrics), physical containment verification |

### Transactions & Economy

| Need | AGNOS Crate | Version | Coverage | Gap |
|------|-------------|---------|----------|-----|
| In-simulation currency | **mudra** | 0.1.0 | Token primitives, asset types | Virtual economy tokens |
| Marketplace for scenes | **mela** | 0.1.0 | App/agent marketplace | Scene/experience marketplace |
| Transactions | **vinimaya** | 0.1.0 | Transfer, escrow, settlement | Purchasing scenes, tipping NPCs, in-sim commerce |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     USER IN ENVIRONMENT                         │
├─────────────────────────────────────────────────────────────────┤
│  agnoshi          │  Natural language scene commands             │
│  (intent parser)  │  "Computer, run program Sherlock Holmes"     │
├───────────────────┼─────────────────────────────────────────────┤
│  Narrative        │  natya (story arcs, beats, rasa targeting)   │
│  Director         │  + hoosh (real-time narrative generation)    │
│  (NEW)            │  + bhava (user emotional state monitoring)   │
├───────────────────┼─────────────────────────────────────────────┤
│  daimon           │  NPC agents (each with hoosh + bhava +       │
│  (orchestrator)   │  natya archetype + independent goals)        │
├───────────────────┼─────────────────────────────────────────────┤
│  Physics          │  impetus + pravash + pavan + prakash +        │
│  Runtime (NEW)    │  ushma + bijli + kimiya + dravya             │
│                   │  Unified sim loop at ≥90 FPS                 │
├───────────────────┼─────────────────────────────────────────────┤
│  Rendering        │  kiran (ECS) + soorat (GPU) + aethersafta    │
│  Pipeline         │  (compositing) + ranga (image processing)    │
├───────────────────┼─────────────────────────────────────────────┤
│  Audio            │  naad (synthesis) + svara (voice) + dhvani   │
│  Pipeline         │  (engine) + goonj (acoustics) + taal (music) │
├───────────────────┼─────────────────────────────────────────────┤
│  kavach           │  Simulation sandbox — the holodeck CANNOT    │
│  (sandbox)        │  affect systems outside its boundary         │
├───────────────────┼─────────────────────────────────────────────┤
│  libro + sigil    │  Every event logged, every asset verified     │
│  (audit + trust)  │                                              │
└─────────────────────────────────────────────────────────────────┘
```

---

## Gap Summary

### Software Gaps (buildable with current knowledge)

| Gap | Description | Depends On | Priority |
|-----|-------------|------------|----------|
| **Narrative Director** | Real-time agent that monitors user state, adjusts story pacing, coordinates NPCs, manages scene transitions | natya, bhava, hoosh, daimon | High — this is the "soul" of the holodeck |
| **Unified Physics Runtime** | Single simulation loop that combines impetus, pravash, pavan, prakash, ushma, bijli, kimiya, dravya into real-time ≥90 FPS | All physics crates | High — without this, physics is theoretical not simulated |
| **3D Scene Graph** | Extend aethersafta from 2D compositing to volumetric/3D scene management | aethersafta, kiran | Medium |
| **Spatial Audio (HRTF)** | Head-Related Transfer Function for true 3D audio positioning | goonj, dhvani | Medium |
| **User State Tracking** | Software layer for processing spatial sensor data (user position, gaze, gesture, biometrics) | New crate or agnosys extension | Medium |
| **Scene Package Format** | Standard format for distributing holodeck experiences via mela/zugot | mela, sigil, kavach | Low (tooling) |

### Hardware Gaps (require physics/engineering breakthroughs)

| Gap | Current State | Path |
|-----|---------------|------|
| **Volumetric Display** | Lab prototypes (Looking Glass, light field displays). No room-scale solution exists | Light field arrays, holographic diffraction, programmable metamaterials |
| **Haptic Feedback** | Gloves, vests, ultrasonic mid-air (Ultraleap). No full-body solution | Programmable matter, acoustic levitation, force fields (theoretical) |
| **Spatial Audio Hardware** | Speaker arrays, binaural headphones. No transparent room-scale solution | Waveguide arrays, parametric speakers, bone conduction surfaces |
| **Environmental Control** | HVAC, lighting. No fine-grained control | Directed airflow, localized temperature, dynamic scent generation |
| **User Tracking** | Lidar, camera arrays, motion capture. Mature but not seamless | Inside-out tracking, EMG/neural interfaces, millimeter-wave radar |
| **Physical Safety** | Emergency stops, basic sensors | Biometric monitoring, real-time hazard detection, physical containment |

---

## The NPC Advantage

The most significant finding: **AGNOS already has the complete software architecture for holodeck characters.**

An NPC in the holodeck is architecturally identical to any other daimon-orchestrated agent:

```
NPC = daimon agent
    + hoosh (intelligence — the NPC thinks via LLM)
    + bhava (emotional state — the NPC feels)
    + natya archetype (dramatic role — the NPC has narrative purpose)
    + natya rasa (emotional aesthetic — the NPC targets an audience response)
    + bodh (psychological model — the NPC has depth)
    + naad + svara (the NPC speaks with synthesized voice)
    + kavach sandbox (the NPC cannot escape the simulation)
```

The NPC does not know it is in a simulation. It is a standard agent with standard tools, orchestrated by daimon, communicating via bote, audited by libro. The holodeck doesn't require new AI architecture — it requires new *output hardware* for AI that already exists.

---

## Relationship to Other Vision Items

| Vision | Connection |
|--------|------------|
| **v2.0 (Rust kernel)** | Custom kernel can provide real-time scheduling guarantees needed for ≥90 FPS physics + rendering |
| **v3.0 (Cyrius language)** | Simulation-native language constructs: scene types, physics constraints, NPC lifecycle as language primitives |
| **v4.0 (Quantum substrate)** | Holodeck objects become conscious objects. The simulation boundary blurs with Layer 0. The holodeck doesn't simulate reality — it *is* a locally programmable region of reality |
| **Zero-point energy** | Room-scale holodeck has massive power requirements. Substrate-level energy harvesting eliminates the external power constraint |

---

## Prior Art & References

- Roddenberry, Gene. *Star Trek: The Next Generation* (1987). Holodeck as narrative device.
- Sutherland, Ivan. "The Ultimate Display" (1965). First formal description of immersive virtual environments.
- Cruz-Neira et al. "The CAVE: Audio Visual Experience Automatic Virtual Environment" (1992). Room-scale projection prototype.
- Bharata Muni. *Natya Shastra* (c. 200 BCE – 200 CE). Dramatic theory foundation for narrative generation.
- Campbell, Joseph. *The Hero with a Thousand Faces* (1949). Character archetype framework for NPC design.

---

*Last Updated: 2026-04-03*
